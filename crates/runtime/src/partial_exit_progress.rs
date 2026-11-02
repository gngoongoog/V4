//! Durable, sequential observation seals for broker-proven TP stages.
//!
//! No price triggers, broker orders, next-stage dispatch or breakeven here.
//! An exact Core-command/MT5-order/Deal audit is necessary but NOT sufficient:
//! only a later full physical-position snapshot proving the resulting volume
//! can seal the current stage. Missing snapshots fail closed.
use anyhow::Result;
use braintrade_domain::Telemetry;
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};

fn context(t:&Telemetry)->String {
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
fn demo(t:&Telemetry,target:&str)->bool{
    target=="demo"&&t.connected&&t.demo&&t.account_trade_mode==Some(0)
        &&t.login>0&&!t.server.is_empty()&&!t.symbol.is_empty()
        &&!t.currency.is_empty()&&t.validate()
}
fn near(a:f64,b:f64,step:f64)->bool{
    a.is_finite()&&b.is_finite()&&step.is_finite()&&step>0.
        &&(a-b).abs()<=step*1e-6
}
pub fn init(db:&Connection)->Result<()>{
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS partial_exit_stage_settlements(
            context TEXT NOT NULL,
            target TEXT NOT NULL CHECK(target='demo'),
            position_identifier INTEGER NOT NULL CHECK(position_identifier>0),
            stage INTEGER NOT NULL CHECK(stage BETWEEN 1 AND 3),
            plan_id TEXT NOT NULL UNIQUE,
            command_id TEXT NOT NULL UNIQUE,
            broker_order INTEGER NOT NULL UNIQUE CHECK(broker_order>0),
            matched_volume REAL NOT NULL CHECK(matched_volume>0),
            remaining_volume REAL NOT NULL CHECK(remaining_volume>=0),
            evidence_json TEXT NOT NULL,
            settled_ms INTEGER NOT NULL CHECK(settled_ms>0),
            PRIMARY KEY(context,target,position_identifier,stage)
         );
         CREATE INDEX IF NOT EXISTS partial_exit_settlements_account
            ON partial_exit_stage_settlements(context,target,position_identifier);")?;
    Ok(())
}
/// Only called AFTER the regular full MT5 snapshot and after original TP
/// drafts/plans have been observed. Never on fast tick path or from UI reads.
pub fn observe(db:&Connection,t:&Telemetry,target:&str,now:u64)->Result<()>{
    if !demo(t,target)||now==0{return Ok(())}
    let key=context(t);
    let links=crate::partial_exit_link::report(db,t,target)?;
    if links["status"]!="broker_stage_links_read_only"{return Ok(())}
    let Some(rows)=links["rows"].as_array()else{return Ok(())};
    // Reading proof outside the transaction is safe: only this Core service
    // owns its SQLite connection. Write uniqueness remains SQL-enforced.
    let mut statement=db.prepare(
        "SELECT position_identifier,ticket,magic,initial_volume,step,payload
         FROM partial_exit_drafts WHERE context=?1 AND target='demo'
         ORDER BY position_identifier LIMIT 100")?;
    let drafts=statement.query_map(params![key],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,
        r.get::<_,u64>(2)?,r.get::<_,f64>(3)?,
        r.get::<_,f64>(4)?,r.get::<_,String>(5)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    for (pid,ticket,magic,initial,step,raw) in drafts{
        let Ok(draft)=serde_json::from_str::<Value>(&raw) else{continue};
        if ticket==0||magic==0||!initial.is_finite()||initial<=0.
            ||!step.is_finite()||step<=0.
            ||draft["position_identifier"].as_u64()!=Some(pid)
            ||draft["ticket"].as_u64()!=Some(ticket)
            ||draft["magic"].as_u64()!=Some(magic)
            ||draft["symbol"]!=t.symbol {continue}
        let mut completed=0.0_f64;
        for stage in 1..=3_u32{
            let Some(row)=rows.iter().find(|row|
                row["position_identifier"].as_u64()==Some(pid)
                &&row["stage"].as_u64()==Some(stage as u64)) else{break};
            let Some(plan)=row["plan_id"].as_str()else{break};
            let Some(volume)=row["planned_volume"].as_f64()
                .filter(|n|n.is_finite()&&*n>0.)else{break};
            let prior:Option<(String,f64)>=db.query_row(
                "SELECT plan_id,matched_volume FROM partial_exit_stage_settlements
                 WHERE context=?1 AND target='demo'
                     AND position_identifier=?2 AND stage=?3",
                params![key,pid,stage],|r|Ok((r.get(0)?,r.get(1)?))
            ).optional()?;
            if let Some((old_plan,old_volume))=prior{
                if old_plan!=plan||!near(old_volume,volume,step){break}
                completed+=volume;
                continue
            }
            // A future stage cannot skip a missing earlier seal.
            if stage>1 {
                let preceding:bool=db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM partial_exit_stage_settlements
                     WHERE context=?1 AND target='demo' AND position_identifier=?2
                        AND stage=?3)",
                    params![key,pid,stage-1],|r|r.get(0))?;
                if !preceding {break}
            }
            let ev=&row["evidence"];
            if ev["state"]!="broker_order_deals_exactly_linked"||
                ev["stage_completed"]!=false||ev["orders_enabled"]!=false||
                !near(ev["matched_broker_lots"].as_f64().unwrap_or(f64::NAN),volume,step)
            {break}
            let Some(id)=ev["core_command_id"].as_str().filter(|id|!id.is_empty())
                else{break};
            let Some(order)=ev["broker_order_ticket"].as_u64().filter(|x|*x>0)
                else{break};
            // One physical position must remain with the *original* owner and
            // expected residual, or be fully absent on TP3.
            let remaining=initial-completed-volume;
            if !remaining.is_finite()||remaining< -step*1e-6{break}
            let existing=t.positions.iter().filter(|p|
                p.identifier==pid||p.ticket==ticket).collect::<Vec<_>>();
            let position_proven=if stage==3 {
                near(remaining,0.,step)&&existing.is_empty()
            }else {
                existing.len()==1 &&existing[0].identifier==pid
                    &&existing[0].ticket==ticket
                    &&existing[0].magic==magic &&existing[0].symbol==t.symbol
                    &&draft["side"].as_str()==Some(existing[0].side.as_str())
                    &&near(existing[0].volume,remaining,step)
            };
            if !position_proven {break}
            // Latching creates no trading permission. It merely remembers the
            // independent historical + full-snapshot evidence through restart.
            let evidence=json!({"schema":"partial-exit-settlement-v1",
                "plan_id":plan,"stage":stage,"command_id":id,"broker_order":order,
                "deal_tickets":ev["broker_deal_tickets"],
                "matched_volume":volume,"remaining_volume":remaining.max(0.),
                "history_until_ms":ev["broker_history_until_ms"],
                "broker_snapshot_ms":now,
                "authorizes_next_stage":false,"breakeven_triggered":false});
            db.execute(
                "INSERT OR IGNORE INTO partial_exit_stage_settlements(
                 context,target,position_identifier,stage,plan_id,command_id,
                 broker_order,matched_volume,remaining_volume,evidence_json,settled_ms)
                 VALUES(?1,'demo',?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![key,pid,stage,plan,id,order,volume,remaining.max(0.),
                    evidence.to_string(),now])?;
            completed+=volume;
        }
    }
    Ok(())
}
pub fn report(db:&Connection,t:&Telemetry,target:&str)->Result<Value>{
    if !demo(t,target){
        return Ok(json!({"schema":"partial-exit-stage-settlements-v1",
            "status":"unverified_demo_account","rows":[],
            "read_only":true,"orders_enabled":false,"breakeven_enabled":false}))
    }
    let key=context(t);
    let mut st=db.prepare(
        "SELECT p.position_identifier,p.stage,p.plan_id,p.planned_volume,
                s.command_id,s.broker_order,s.matched_volume,s.remaining_volume,
                s.settled_ms
         FROM partial_exit_stage_plans p
         LEFT JOIN partial_exit_stage_settlements s
           ON p.context=s.context AND p.target=s.target
           AND p.position_identifier=s.position_identifier AND p.stage=s.stage
           AND p.plan_id=s.plan_id
         WHERE p.context=?1 AND p.target='demo'
         ORDER BY p.created_ms DESC,p.position_identifier DESC,p.stage LIMIT 75")?;
    let rows=st.query_map(params![key],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,u32>(1)?,r.get::<_,String>(2)?,
        r.get::<_,f64>(3)?,r.get::<_,Option<String>>(4)?,
        r.get::<_,Option<u64>>(5)?,r.get::<_,Option<f64>>(6)?,
        r.get::<_,Option<f64>>(7)?,r.get::<_,Option<u64>>(8)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut result=Vec::new();
    for (pid,stage,plan,lot,id,order,filled,left,settled) in rows{
        result.push(json!({"position_identifier":pid,"stage":stage,"plan_id":plan,
            "planned_volume":lot,
            "state":if settled.is_some(){"historical_stage_sealed"}else{"unsettled"},
            "core_command_id":id,"broker_order":order,"matched_volume":filled,
            "remaining_volume_at_seal":left,"sealed_ms":settled,
            "next_stage_authorized":false,"breakeven_triggered":false}));
    }
    Ok(json!({"schema":"partial-exit-stage-settlements-v1",
        "status":"account_scoped_historical_settlements",
        "context":key,"target":"demo","rows":result,
        "read_only":true,"orders_enabled":false,"next_stage_authorized":false,
        "breakeven_enabled":false,
        "note":"A sealed stage confirms exact broker Deal history AND a reconciled full position snapshot. It does not issue any future order or activate breakeven."}))
}
