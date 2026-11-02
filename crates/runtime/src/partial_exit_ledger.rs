//! Durable, read-only accounting of *draft* three-target partial-exit splits.
//!
//! A draft is neither an order nor evidence that any price target, exit, or
//! broker deal happened. Only normal reconciled MT5 snapshots touch this table.
//! Never call from Sniper Fast Pulse, and never dispatch from this module.
use anyhow::Result;
use braintrade_domain::{execution, partial_exit, Telemetry};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

fn context(t:&Telemetry)->String {
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
fn bound(t:&Telemetry,target:&str)->bool{
    t.connected && t.login>0 && !t.server.is_empty()
        && !t.currency.is_empty() && !t.symbol.is_empty()
        && matches!((target,t.demo,t.account_trade_mode),
            ("demo",true,Some(0))|("live",false,Some(2)))
}

pub fn init(db:&Connection)->Result<()>{
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS partial_exit_drafts(
           context TEXT NOT NULL,
           target TEXT NOT NULL CHECK(target IN ('demo','live')),
           position_identifier INTEGER NOT NULL CHECK(position_identifier>0),
           ticket INTEGER NOT NULL CHECK(ticket>0),
           magic INTEGER NOT NULL,
           initial_volume REAL NOT NULL CHECK(initial_volume>0),
           step REAL NOT NULL CHECK(step>0),
           created_ms INTEGER NOT NULL,
           last_seen_ms INTEGER NOT NULL,
           status TEXT NOT NULL CHECK(status IN
                ('draft_only','volume_changed_unverified','position_missing_unverified')),
           payload TEXT NOT NULL,
           PRIMARY KEY(context,target,position_identifier)
         );
         CREATE INDEX IF NOT EXISTS partial_exit_drafts_recent
            ON partial_exit_drafts(context,target,created_ms DESC);"
    )?;
    Ok(())
}

/// Durable snapshot-based drafts. No broker commands, closes or generated
/// prices. Existing rows can only stay draft or become explicitly unverified;
/// they never become broker-completed and are never silently re-created.
pub fn observe(db:&Connection,t:&Telemetry,target:&str,now:u64)->Result<()>{
    if !bound(t,target)||now==0||!t.validate(){return Ok(())}
    let key=context(t);
    let tx=db.unchecked_transaction()?;
    let step=t.volume_step.unwrap_or(0.);
    let min=t.volume_min.unwrap_or(0.);
    let max=t.volume_max.unwrap_or(0.);
    for p in &t.positions {
        if p.symbol!=t.symbol || p.ticket==0||p.identifier==0
            || !matches!(p.side.as_str(),"BUY"|"SELL")
            || execution::brain_for_magic(p.magic).is_none()
            || !p.volume.is_finite() || p.volume<=0. {continue}
        let prior=tx.query_row(
            "SELECT ticket,magic,initial_volume,step,status
             FROM partial_exit_drafts
             WHERE context=?1 AND target=?2 AND position_identifier=?3",
            params![key,target,p.identifier],
            |r|Ok((r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,
                r.get::<_,f64>(2)?,r.get::<_,f64>(3)?,r.get::<_,String>(4)?)),
        );
        match prior {
            Ok((ticket,magic,initial,original_step,state))=>{
                // A resized or unexpectedly reused identity must never
                // automatically gain another clean draft on later snapshots.
                let unchanged=ticket==p.ticket && magic==p.magic
                    && step.is_finite()&&step>0. && (step-original_step).abs()<=original_step*1e-8
                    && (p.volume-initial).abs()<=original_step*1e-6;
                let next=if state=="draft_only"&&unchanged {"draft_only"}
                    else {"volume_changed_unverified"};
                tx.execute(
                    "UPDATE partial_exit_drafts SET status=?4,last_seen_ms=?5
                     WHERE context=?1 AND target=?2 AND position_identifier=?3",
                    params![key,target,p.identifier,next,now])?;
            }
            Err(rusqlite::Error::QueryReturnedNoRows)=>{
                // Broker data must prove all three steps legal before a
                // draft is persisted. No settings inference or rounding up.
                if let Ok(split)=partial_exit::three_target_lots(p.volume,min,step,max){
                    let payload=json!({
                        "schema":"partial-exit-draft-v1",
                        "weights_percent":[50,30,20],
                        "lots":split,
                        "position_identifier":p.identifier,
                        "ticket":p.ticket,
                        "magic":p.magic,
                        "symbol":p.symbol,
                        "side":p.side,
                        "entry_volume":p.volume,
                        "volume_step":step,
                        "created_ms":now,
                        "broker_orders_created":false,
                        "targets_priced":false
                    });
                    tx.execute(
                        "INSERT INTO partial_exit_drafts(
                            context,target,position_identifier,ticket,magic,
                            initial_volume,step,created_ms,last_seen_ms,status,payload
                         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8,'draft_only',?9)",
                        params![key,target,p.identifier,p.ticket,p.magic,p.volume,step,now,payload.to_string()])?;
                }
            }
            Err(error)=>return Err(error.into()),
        }
    }
    // An absent position is NOT a verified exit. Preserve this uncertainty
    // and all three original stage volumes across restarts.
    let mut st=tx.prepare(
        "SELECT position_identifier FROM partial_exit_drafts
         WHERE context=?1 AND target=?2 AND status='draft_only'")?;
    let ids=st.query_map(params![key,target],|r|r.get::<_,u64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(st);
    for id in ids {
        if !t.positions.iter().any(|p|p.identifier==id&&p.symbol==t.symbol){
            tx.execute(
                "UPDATE partial_exit_drafts SET status='position_missing_unverified'
                 WHERE context=?1 AND target=?2 AND position_identifier=?3
                   AND status='draft_only'",
                params![key,target,id])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Read-only account-scoped evidence. The caller must separately require
/// fresh bound MT5 telemetry and exact execution account identity.
pub fn report(db:&Connection,t:&Telemetry,target:&str)->Result<Value>{
    if !bound(t,target){
        return Ok(json!({"schema":"partial-exit-draft-ledger-v1",
            "status":"unverified_account","read_only":true,
            "orders_enabled":false,"rows":[]}));
    }
    let key=context(t);
    let mut statement=db.prepare(
        "SELECT position_identifier,ticket,initial_volume,created_ms,last_seen_ms,status,payload
         FROM partial_exit_drafts WHERE context=?1 AND target=?2
         ORDER BY created_ms DESC,position_identifier DESC LIMIT 25")?;
    let rows=statement.query_map(params![key,target],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,
        r.get::<_,f64>(2)?,r.get::<_,u64>(3)?,r.get::<_,u64>(4)?,
        r.get::<_,String>(5)?,r.get::<_,String>(6)?,
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut results=Vec::new();
    for (pid,ticket,volume,created,seen,state,raw) in rows{
        let payload=serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
        // A damaged journal must never be displayed as validated TP sizing.
        let intact=payload["schema"]=="partial-exit-draft-v1"
            && payload["position_identifier"].as_u64()==Some(pid)
            && payload["ticket"].as_u64()==Some(ticket)
            && payload["entry_volume"].as_f64()==Some(volume)
            && payload["broker_orders_created"]==false;
        let display_state=if intact {state.as_str()}else{"invalid_draft_journal"};
        results.push(json!({
            "position_identifier":pid,"ticket":ticket,"initial_volume":volume,
            "created_ms":created,"last_seen_ms":seen,
            "state":display_state,
            "lots":if intact {payload["lots"].clone()}else{Value::Null}
        }));
    }
    Ok(json!({
        "schema":"partial-exit-draft-ledger-v1",
        "status":"research_drafts_only",
        "context":key,"target":target,"read_only":true,
        "orders_enabled":false,
        "source":"reconciled broker position snapshots (not deal history)",
        "rows":results,
        "note":"Draft only. Missing positions, changed size, or restart do not prove TP fill; no partial close is dispatched by this ledger."
    }))
}

#[cfg(test)]
mod tests{
    use super::*;
    use braintrade_domain::Position;
    fn db()->Connection{let d=Connection::open_in_memory().unwrap();init(&d).unwrap();d}
    fn t()->Telemetry{
        Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login:15,server:"D".into(),currency:"USD".into(),symbol:"XAUUSDm".into(),
            volume_min:Some(0.01),volume_step:Some(0.01),volume_max:Some(100.),
            positions:vec![Position{ticket:9,identifier:42,magic:410601,
                symbol:"XAUUSDm".into(),side:"BUY".into(),volume:0.1,..Default::default()}],
            ..Default::default()}
    }
    #[test]
    fn persists_draft_without_commands_and_restarts_idempotently(){
        let d=db();let t=t();
        observe(&d,&t,"demo",100).unwrap();
        observe(&d,&t,"demo",101).unwrap();
        let p=report(&d,&t,"demo").unwrap();
        assert_eq!(p["rows"].as_array().unwrap().len(),1);
        assert_eq!(p["rows"][0]["state"],"draft_only");
        assert_eq!(p["rows"][0]["lots"]["tp1"],0.05);
        assert_eq!(p["rows"][0]["lots"]["tp2"],0.03);
        assert_eq!(p["rows"][0]["lots"]["tp3"],0.02);
        init(&d).unwrap();
        assert_eq!(report(&d,&t,"demo").unwrap()["rows"][0]["state"],"draft_only");
        assert_eq!(d.query_row("SELECT COUNT(*) FROM partial_exit_drafts",[],|r|r.get::<_,u64>(0)).unwrap(),1);
        assert_eq!(p["orders_enabled"],false);
    }
    #[test]
    fn position_change_or_disappearance_can_never_be_interpreted_as_filled(){
        let d=db();let mut t=t();
        observe(&d,&t,"demo",100).unwrap();
        t.positions[0].volume=0.05;observe(&d,&t,"demo",200).unwrap();
        assert_eq!(report(&d,&t,"demo").unwrap()["rows"][0]["state"],"volume_changed_unverified");
        t.positions[0].volume=0.1;observe(&d,&t,"demo",300).unwrap();
        assert_eq!(report(&d,&t,"demo").unwrap()["rows"][0]["state"],"volume_changed_unverified");
        let d=db();let mut t=t();observe(&d,&t,"demo",100).unwrap();
        t.positions.clear();observe(&d,&t,"demo",300).unwrap();
        assert_eq!(report(&d,&t,"demo").unwrap()["rows"][0]["state"],"position_missing_unverified");
        t.positions.push(t_zero_position());
        observe(&d,&t,"demo",350).unwrap();
        assert_ne!(report(&d,&t,"demo").unwrap()["rows"][0]["state"],"draft_only");
    }
    fn t_zero_position()->Position{t().positions.remove(0)}
    #[test]
    fn cross_account_unverified_and_ineligible_small_volume_do_not_leak(){
        let d=db();let mut t=t();t.positions[0].volume=0.02;
        observe(&d,&t,"demo",100).unwrap();
        assert!(report(&d,&t,"demo").unwrap()["rows"].as_array().unwrap().is_empty());
        t.positions[0].volume=0.1;observe(&d,&t,"demo",200).unwrap();
        assert_eq!(report(&d,&t,"demo").unwrap()["rows"].as_array().unwrap().len(),1);
        let mut other=t.clone();other.login=16;
        assert!(report(&d,&other,"demo").unwrap()["rows"].as_array().unwrap().is_empty());
        other.connected=false;
        assert_eq!(report(&d,&other,"demo").unwrap()["status"],"unverified_account");
        other=t.clone();other.demo=false;other.account_trade_mode=Some(2);
        assert!(report(&d,&other,"live").unwrap()["rows"].as_array().unwrap().is_empty());
    }
    #[test]
    fn invalid_and_mismatched_source_does_not_mutate_journal(){
        let d=db();let mut t=t();t.volume_step=None;
        observe(&d,&t,"demo",100).unwrap();
        assert!(report(&d,&t,"demo").unwrap()["rows"].as_array().unwrap().is_empty());
        t.volume_step=Some(0.01);t.connected=false;
        observe(&d,&t,"demo",200).unwrap();
        assert!(report(&d,&t,"demo").unwrap()["rows"].as_array().unwrap().is_empty());
    }
}
