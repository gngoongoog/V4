//! Immutable three-stage TP *plan identities*. This module creates only
//! research/planning records and deliberately contains NO broker dispatch.
//!
//! Reserved stage IDs are NOT broker order IDs. They must never be sent to
//! Gateway or treated as evidence of a close, a fill, or a price target.
use anyhow::Result;
use braintrade_domain::{execution, partial_exit, Telemetry};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

fn context(t:&Telemetry)->String{
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
fn bound(t:&Telemetry,target:&str)->bool{
    t.connected && t.login>0 && !t.server.is_empty()
        && !t.symbol.is_empty() && !t.currency.is_empty()
        && matches!((target,t.demo,t.account_trade_mode),
            ("demo",true,Some(0))|("live",false,Some(2)))
}
pub fn init(db:&Connection)->Result<()>{
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS partial_exit_stage_plans(
           context TEXT NOT NULL,
           target TEXT NOT NULL CHECK(target IN ('demo','live')),
           position_identifier INTEGER NOT NULL CHECK(position_identifier>0),
           stage INTEGER NOT NULL CHECK(stage BETWEEN 1 AND 3),
           plan_id TEXT NOT NULL UNIQUE,
           planned_volume REAL NOT NULL CHECK(planned_volume>0),
           created_ms INTEGER NOT NULL,
           status TEXT NOT NULL CHECK(status IN ('prepared_only','review_required')),
           PRIMARY KEY(context,target,position_identifier,stage)
         );
         CREATE INDEX IF NOT EXISTS partial_exit_stage_plans_recent
           ON partial_exit_stage_plans(context,target,created_ms DESC);")?;
    Ok(())
}
fn near(a:f64,b:f64,step:f64)->bool{
    a.is_finite() && b.is_finite() && step.is_finite() && step>0.
        && (a-b).abs()<=step*1e-6
}
/// Only on reconciled full snapshots and AFTER the draft ledger observer.
/// Stages are immutable plans, not outbound commands, and never transition to
/// filled. A change/missing center permanently revokes all prepared stages.
pub fn observe(db:&Connection,t:&Telemetry,target:&str,now:u64)->Result<()>{
    if !bound(t,target)||now==0||!t.validate(){return Ok(())}
    let key=context(t);
    let tx=db.unchecked_transaction()?;
    // Any uncertain original stage draft revokes its corresponding prepared
    // reservations; no later snapshot can silently restore them.
    tx.execute(
        "UPDATE partial_exit_stage_plans SET status='review_required'
         WHERE context=?1 AND target=?2 AND status='prepared_only' AND
           EXISTS(SELECT 1 FROM partial_exit_drafts d
                  WHERE d.context=partial_exit_stage_plans.context
                    AND d.target=partial_exit_stage_plans.target
                    AND d.position_identifier=partial_exit_stage_plans.position_identifier
                    AND d.status<>'draft_only')",
        params![key,target])?;
    let mut statement=tx.prepare(
        "SELECT position_identifier,ticket,magic,initial_volume,step,payload
         FROM partial_exit_drafts
         WHERE context=?1 AND target=?2 AND status='draft_only'
         ORDER BY created_ms DESC LIMIT 100")?;
    let entries=statement.query_map(params![key,target],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,r.get::<_,u64>(2)?,
        r.get::<_,f64>(3)?,r.get::<_,f64>(4)?,r.get::<_,String>(5)?,
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    for (pid,ticket,magic,original,step,raw) in entries {
        let Some(p)=t.positions.iter().find(|p|p.identifier==pid && p.ticket==ticket
            &&p.magic==magic &&p.symbol==t.symbol
            &&matches!(p.side.as_str(),"BUY"|"SELL")
            &&execution::brain_for_magic(p.magic).is_some()
            &&near(p.volume,original,step)) else {continue};
        let Ok(original_payload)=serde_json::from_str::<Value>(&raw) else {continue};
        if original_payload["schema"]!="partial-exit-draft-v1"
            || original_payload["position_identifier"].as_u64()!=Some(pid)
            || original_payload["ticket"].as_u64()!=Some(ticket)
            || original_payload["magic"].as_u64()!=Some(magic)
            || original_payload["symbol"].as_str()!=Some(p.symbol.as_str())
            || original_payload["side"].as_str()!=Some(p.side.as_str())
            || original_payload["broker_orders_created"]!=false
            || !near(original_payload["entry_volume"].as_f64().unwrap_or(f64::NAN),original,step)
            || !near(original_payload["volume_step"].as_f64().unwrap_or(f64::NAN),step,step) {continue}
        let (Some(min),Some(max),Some(current_step))=
            (t.volume_min,t.volume_max,t.volume_step) else {continue};
        if !near(step,current_step,step){continue}
        let Ok(lots)=partial_exit::three_target_lots(original,min,step,max) else {continue};
        let expected=[lots.tp1,lots.tp2,lots.tp3];
        if !["tp1","tp2","tp3"].iter().enumerate().all(|(i,name)|
            near(original_payload["lots"][*name].as_f64().unwrap_or(f64::NAN),
                expected[i],step)){continue}
        for (i,volume) in expected.into_iter().enumerate(){
            let stage=(i+1) as u32;
            let plan_id=format!("tp-plan-{}",uuid::Uuid::new_v4().simple());
            // The composite primary key is the idempotency guarantee,
            // including after Core restart. No mutable order volume/state.
            tx.execute(
                "INSERT OR IGNORE INTO partial_exit_stage_plans(
                    context,target,position_identifier,stage,plan_id,
                    planned_volume,created_ms,status)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,'prepared_only')",
                params![key,target,pid,stage,plan_id,volume,now])?;
        }
    }
    tx.commit()?;
    Ok(())
}
pub fn report(db:&Connection,t:&Telemetry,target:&str)->Result<Value>{
    if !bound(t,target){
        return Ok(json!({"schema":"partial-exit-stage-plans-v1","status":"unverified_account",
            "read_only":true,"orders_enabled":false,"rows":[]}));
    }
    let key=context(t);
    let mut stmt=db.prepare(
        "SELECT position_identifier,stage,plan_id,planned_volume,created_ms,status
         FROM partial_exit_stage_plans WHERE context=?1 AND target=?2
         ORDER BY created_ms DESC,position_identifier DESC,stage ASC LIMIT 75")?;
    let rows=stmt.query_map(params![key,target],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,u32>(1)?,r.get::<_,String>(2)?,
        r.get::<_,f64>(3)?,r.get::<_,u64>(4)?,r.get::<_,String>(5)?,
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let items=rows.into_iter().map(|(pid,stage,id,volume,created,state)|json!({
        "position_identifier":pid,"stage":stage,"plan_id":id,
        "planned_volume":volume,"created_ms":created,"state":state
    })).collect::<Vec<_>>();
    Ok(json!({
        "schema":"partial-exit-stage-plans-v1","context":key,"target":target,
        "status":"draft_identities_only",
        "read_only":true,"orders_enabled":false,
        "stage_execution_supported":false,
        "rows":items,
        "note":"Stable TP plan identifiers only; no broker send, targets, close, fills or stage completion."
    }))
}

#[cfg(test)]
mod tests{
    use super::*;
    use braintrade_domain::Position;
    fn db()->Connection{
        let d=Connection::open_in_memory().unwrap();
        crate::partial_exit_ledger::init(&d).unwrap();init(&d).unwrap();d
    }
    fn tele()->Telemetry{
        Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login:222,server:"DemoA".into(),currency:"USD".into(),
            symbol:"XAUUSDm".into(),volume_min:Some(0.01),
            volume_step:Some(0.01),volume_max:Some(100.),
            positions:vec![Position{ticket:87,identifier:907,magic:410601,
                symbol:"XAUUSDm".into(),side:"SELL".into(),
                volume:0.1,..Default::default()}],..Default::default()
        }
    }
    fn capture(db:&Connection,t:&Telemetry,now:u64){
        crate::partial_exit_ledger::observe(db,t,"demo",now).unwrap();
        observe(db,t,"demo",now).unwrap();
    }
    #[test]
    fn exactly_three_identities_are_immutable_and_repeated_snapshots_cannot_duplicate(){
        let d=db();let t=tele();
        capture(&d,&t,100);let first=report(&d,&t,"demo").unwrap();
        let rows=first["rows"].as_array().unwrap();
        assert_eq!(rows.len(),3);
        assert_eq!(rows[0]["planned_volume"],0.05);
        assert_eq!(rows[1]["planned_volume"],0.03);
        assert_eq!(rows[2]["planned_volume"],0.02);
        assert!(rows.iter().all(|v|v["state"]=="prepared_only"));
        assert!(rows.iter().all(|v|v["plan_id"].as_str().unwrap().starts_with("tp-plan-")));
        assert_eq!(first["orders_enabled"],false);
        capture(&d,&t,200);
        assert_eq!(report(&d,&t,"demo").unwrap(),json!({
            "schema":"partial-exit-stage-plans-v1","context":context(&t),"target":"demo",
            "status":"draft_identities_only","read_only":true,"orders_enabled":false,
            "stage_execution_supported":false,"rows":rows,
            "note":"Stable TP plan identifiers only; no broker send, targets, close, fills or stage completion."
        }));
        init(&d).unwrap();capture(&d,&t,300);
        assert_eq!(report(&d,&t,"demo").unwrap()["rows"],first["rows"]);
    }
    #[test]
    fn any_volume_change_or_missing_position_revokes_three_stage_plans_forever(){
        let d=db();let mut t=tele();capture(&d,&t,100);
        t.positions[0].volume=0.07;capture(&d,&t,200);
        let after=report(&d,&t,"demo").unwrap();
        assert!(after["rows"].as_array().unwrap().iter()
            .all(|r|r["state"]=="review_required"));
        t.positions[0].volume=0.1;capture(&d,&t,300);
        assert_eq!(report(&d,&t,"demo").unwrap()["rows"],after["rows"]);
        let d=db();let mut t=tele();capture(&d,&t,100);
        t.positions.clear();capture(&d,&t,200);
        assert!(report(&d,&t,"demo").unwrap()["rows"].as_array().unwrap()
            .iter().all(|r|r["state"]=="review_required"));
    }
    #[test]
    fn small_position_and_foreign_account_do_not_gain_stage_plans(){
        let d=db();let mut t=tele();t.positions[0].volume=0.02;
        capture(&d,&t,100);
        assert_eq!(report(&d,&t,"demo").unwrap()["rows"].as_array().unwrap().len(),0);
        t.positions[0].volume=0.1;
        capture(&d,&t,200);
        assert_eq!(report(&d,&t,"demo").unwrap()["rows"].as_array().unwrap().len(),3);
        let mut foreign=t.clone();foreign.login=223;
        assert_eq!(report(&d,&foreign,"demo").unwrap()["rows"].as_array().unwrap().len(),0);
        foreign.connected=false;
        assert_eq!(report(&d,&foreign,"demo").unwrap()["status"],"unverified_account");
    }
}