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
// Re-validate a latched settlement against the current immutable broker
// receipts. Historical storage must not become proof after source corruption.
fn receipt_matches(ev:&Value,volume:f64,step:f64,plan:&str,
    id:&str,order:u64,old:&Value)->bool{
    ev["state"]=="broker_order_deals_exactly_linked"
        &&ev["stage_completed"]==false&&ev["orders_enabled"]==false
        &&ev["core_command_id"]==id&&ev["broker_order_ticket"].as_u64()==Some(order)
        &&near(ev["matched_broker_lots"].as_f64().unwrap_or(f64::NAN),volume,step)
        &&old["schema"]=="partial-exit-settlement-v1"&&old["plan_id"]==plan
        &&old["command_id"]==id&&old["broker_order"].as_u64()==Some(order)
        &&old["deal_tickets"]==ev["broker_deal_tickets"]
        &&near(old["matched_volume"].as_f64().unwrap_or(f64::NAN),volume,step)
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
            broker_order INTEGER NOT NULL CHECK(broker_order>0),
            matched_volume REAL NOT NULL CHECK(matched_volume>0),
            remaining_volume REAL NOT NULL CHECK(remaining_volume>=0),
            evidence_json TEXT NOT NULL,
            settled_ms INTEGER NOT NULL CHECK(settled_ms>0),
            PRIMARY KEY(context,target,position_identifier,stage),
            UNIQUE(context,target,broker_order)
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
            let prior:Option<(String,f64,String,String,u64)>=db.query_row(
                "SELECT plan_id,matched_volume,evidence_json,command_id,broker_order
                 FROM partial_exit_stage_settlements
                 WHERE context=?1 AND target='demo'
                     AND position_identifier=?2 AND stage=?3",
                params![key,pid,stage],|r|Ok((
                    r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?
                ))
            ).optional()?;
            if let Some((old_plan,old_volume,old_raw,old_id,old_order))=prior{
                let old=serde_json::from_str::<Value>(&old_raw).unwrap_or(Value::Null);
                if old_plan!=plan||!near(old_volume,volume,step)||
                    !receipt_matches(&row["evidence"],volume,step,plan,
                        &old_id,old_order,&old) {break}
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
            // Floating representation may leave 3e-18 after 0.10-0.05-0.03-0.02.
            // Normalize only if demonstrably zero on the broker's lot grid.
            let certified_remaining=if near(remaining,0.,step){0.0}else{remaining};
            // Latching creates no trading permission. It merely remembers the
            // independent historical + full-snapshot evidence through restart.
            let evidence=json!({"schema":"partial-exit-settlement-v1",
                "plan_id":plan,"stage":stage,"command_id":id,"broker_order":order,
                "deal_tickets":ev["broker_deal_tickets"],
                "matched_volume":volume,"remaining_volume":certified_remaining,
                "history_until_ms":ev["broker_history_until_ms"],
                "broker_snapshot_ms":now,
                "authorizes_next_stage":false,"breakeven_triggered":false});
            db.execute(
                "INSERT OR IGNORE INTO partial_exit_stage_settlements(
                 context,target,position_identifier,stage,plan_id,command_id,
                 broker_order,matched_volume,remaining_volume,evidence_json,settled_ms)
                 VALUES(?1,'demo',?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![key,pid,stage,plan,id,order,volume,certified_remaining,
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
                s.settled_ms,s.evidence_json
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
        r.get::<_,Option<f64>>(7)?,r.get::<_,Option<u64>>(8)?,
        r.get::<_,Option<String>>(9)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let links=crate::partial_exit_link::report(db,t,target)?;
    let current=links["rows"].as_array();
    let mut result=Vec::new();
    for (pid,stage,plan,lot,id,order,filled,left,settled,old_raw) in rows{
        let still_proven=if let (Some(id),Some(order),Some(old_raw),
            Some(step))=(&id,order,&old_raw,t.volume_step){
            let old=serde_json::from_str::<Value>(old_raw).unwrap_or(Value::Null);
            current.and_then(|rows|rows.iter().find(|r|
                r["position_identifier"].as_u64()==Some(pid)
                &&r["stage"].as_u64()==Some(stage as u64)
                &&r["plan_id"]==plan)).is_some_and(|r|
                    receipt_matches(&r["evidence"],lot,step,&plan,id,order,&old))
        }else{false};
        let state=if settled.is_some(){
            if still_proven{"historical_stage_sealed"}else{"sealed_evidence_needs_review"}
        }else{"unsettled"};
        result.push(json!({"position_identifier":pid,"stage":stage,"plan_id":plan,
            "planned_volume":lot,
            "state":state,
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


#[cfg(test)]
mod tests{
    use super::*;
    use braintrade_domain::Position;
    fn tele()->Telemetry{
        Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login:222,server:"DemoA".into(),currency:"USD".into(),
            symbol:"XAUUSDm".into(),volume_min:Some(0.01),
            volume_step:Some(0.01),volume_max:Some(100.),
            positions:vec![Position{ticket:87,identifier:907,magic:410601,
                symbol:"XAUUSDm".into(),side:"BUY".into(),volume:0.10,
                ..Default::default()}],..Default::default()}
    }
    fn fixture(d:&Connection){
        crate::partial_exit_ledger::init(d).unwrap();
        crate::partial_exit_stage::init(d).unwrap();
        init(d).unwrap();
        d.execute_batch(
            "CREATE TABLE broker_commands(id TEXT PRIMARY KEY,context TEXT,brain TEXT,
                kind TEXT,created_ms INTEGER,state TEXT,payload TEXT,result TEXT);
             CREATE TABLE broker_deals(context TEXT,ticket INTEGER,position_id INTEGER,
                brain TEXT,payload TEXT,PRIMARY KEY(context,ticket));
             CREATE TABLE broker_history_sync(context TEXT,target TEXT,until_ms INTEGER,
                synced_ms INTEGER,PRIMARY KEY(context,target));").unwrap();
        let t=tele();
        crate::partial_exit_ledger::observe(d,&t,"demo",100).unwrap();
        crate::partial_exit_stage::observe(d,&t,"demo",100).unwrap();
        d.execute("INSERT INTO broker_history_sync VALUES(?1,'demo',1000,1001)",
            [context(&t)]).unwrap();
    }
    fn seed(d:&Connection,stage:u32){
        let t=tele();let key=context(&t);
        let plans=crate::partial_exit_stage::report(d,&t,"demo").unwrap();
        let row=&plans["rows"][(stage-1) as usize];
        let plan=row["plan_id"].as_str().unwrap();
        let quantity=[0.05,0.03,0.02][(stage-1) as usize];
        let expected=[0.1,0.05,0.02][(stage-1) as usize];
        let cmd_id=format!("core-close-{stage}");
        let order=700+stage as u64;
        let ticket=900+stage as u64;
        let cmd=json!({"id":cmd_id,"kind":"close","context":key,
            "target":"demo","login":222,"server":"DemoA","currency":"USD",
            "symbol":"XAUUSDm","brain":"gann","magic":410601,
            "position":87,"tp_stage":stage,"tp_stage_plan_id":plan,
            "lots":quantity,"created_ms":150+stage as u64,
            "expected_position_identifier":907,
            "expected_position_volume":expected,"expected_position_side":"BUY"});
        let receipt=json!({"state":"filled","recovered_from_history":true,
            "volume":quantity,"position_id":907,"order":order,
            "deal":ticket,"deals":[ticket],
            "request":{"volume":quantity,"position":87,"type":1,
                "magic":410601,"symbol":"XAUUSDm"}});
        d.execute("INSERT INTO broker_commands VALUES(?1,?2,'gann','close',?3,
                'observed',?4,?5)",
            params![cmd_id,key,150+stage as u64,cmd.to_string(),receipt.to_string()]).unwrap();
        let deal=json!({"ticket":ticket,"order":order,"position_id":907,
            "symbol":"XAUUSDm","magic":410601,"entry":1,"type":1,
            "time_msc":200+stage as u64,"price":2000.0,"volume":quantity,
            "profit":1.0,"commission":-0.03,"swap":0.0,"fee":0.0});
        d.execute("INSERT INTO broker_deals VALUES(?1,?2,907,'gann',?3)",
            params![key,ticket,deal.to_string()]).unwrap();
    }
    fn snapshot(d:&Connection,t:&Telemetry,now:u64){
        crate::partial_exit_ledger::observe(d,t,"demo",now).unwrap();
        crate::partial_exit_stage::observe(d,t,"demo",now).unwrap();
        observe(d,t,"demo",now).unwrap();
    }
    fn row(d:&Connection,t:&Telemetry,i:usize)->Value{
        report(d,t,"demo").unwrap()["rows"][i].clone()
    }
    #[test]
    fn sequential_stage_seals_require_actual_remaining_broker_position(){
        let d=Connection::open_in_memory().unwrap();fixture(&d);
        let mut t=tele();seed(&d,1);
        observe(&d,&t,"demo",200).unwrap();
        assert_eq!(row(&d,&t,0)["state"],"unsettled",
            "An exact broker Deal without a changed full position cannot seal TP1");
        t.positions[0].volume=0.06;
        snapshot(&d,&t,210);
        assert_eq!(row(&d,&t,0)["state"],"unsettled");
        t.positions[0].volume=0.05;
        snapshot(&d,&t,220);
        assert_eq!(row(&d,&t,0)["state"],"historical_stage_sealed");
        assert_eq!(row(&d,&t,0)["remaining_volume_at_seal"],0.05);
        assert_eq!(row(&d,&t,1)["state"],"unsettled");
        seed(&d,2);
        observe(&d,&t,"demo",230).unwrap();
        assert_eq!(row(&d,&t,1)["state"],"unsettled");
        t.positions[0].volume=0.02;
        snapshot(&d,&t,240);
        assert_eq!(row(&d,&t,1)["state"],"historical_stage_sealed");
        seed(&d,3);
        observe(&d,&t,"demo",250).unwrap();
        assert_eq!(row(&d,&t,2)["state"],"unsettled");
        t.positions.clear();
        snapshot(&d,&t,260);
        assert_eq!(row(&d,&t,2)["state"],"historical_stage_sealed");
        assert_eq!(row(&d,&t,2)["remaining_volume_at_seal"],0.0);
        let v=report(&d,&t,"demo").unwrap();
        assert_eq!(v["orders_enabled"],false);
        assert_eq!(v["next_stage_authorized"],false);
        assert_eq!(v["breakeven_enabled"],false);
        assert!(v["rows"].as_array().unwrap().iter()
            .all(|r|r["next_stage_authorized"]==false
                &&r["breakeven_triggered"]==false));
    }
    #[test]
    fn absent_intermediate_snapshot_never_skips_to_later_stage(){
        let d=Connection::open_in_memory().unwrap();fixture(&d);
        seed(&d,1);seed(&d,2);
        let mut t=tele();t.positions[0].volume=0.02;
        snapshot(&d,&t,300);
        assert_eq!(row(&d,&t,0)["state"],"unsettled");
        assert_eq!(row(&d,&t,1)["state"],"unsettled");
        assert_eq!(d.query_row("SELECT COUNT(*) FROM partial_exit_stage_settlements",[],
            |r|r.get::<_,u32>(0)).unwrap(),0);
    }
    #[test]
    fn invalid_broker_deal_or_wrong_owner_never_seals(){
        let d=Connection::open_in_memory().unwrap();fixture(&d);
        let mut t=tele();seed(&d,1);t.positions[0].volume=0.05;
        d.execute("UPDATE broker_deals SET payload=json_set(payload,'$.order',12345)",[]).unwrap();
        snapshot(&d,&t,200);
        assert_eq!(row(&d,&t,0)["state"],"unsettled");
        d.execute("UPDATE broker_deals SET payload=json_set(payload,'$.order',701)",[]).unwrap();
        t.positions[0].magic=999;
        snapshot(&d,&t,300);
        assert_eq!(row(&d,&t,0)["state"],"unsettled");
    }
    #[test]
    fn previously_sealed_stage_cannot_unlock_next_when_history_was_corrupted(){
        let d=Connection::open_in_memory().unwrap();fixture(&d);
        let mut t=tele();seed(&d,1);t.positions[0].volume=0.05;
        snapshot(&d,&t,200);
        assert_eq!(row(&d,&t,0)["state"],"historical_stage_sealed");
        d.execute("UPDATE broker_deals SET payload=json_set(payload,'$.order',999)",[]).unwrap();
        assert_eq!(row(&d,&t,0)["state"],"sealed_evidence_needs_review");
        seed(&d,2);t.positions[0].volume=0.02;
        snapshot(&d,&t,300);
        assert_eq!(row(&d,&t,1)["state"],"unsettled",
            "TP2 may not be sealed when TP1 broker order proof is now corrupted");
        assert_eq!(row(&d,&t,0)["state"],"sealed_evidence_needs_review");
    }
    #[test]
    fn immutable_seal_survives_file_restart_and_foreign_identity_cannot_read(){
        let path=std::env::temp_dir().join(format!("bt-tp-seal-{}.sqlite",
            uuid::Uuid::new_v4().simple()));
        let mut t=tele();t.positions[0].volume=0.05;
        {
            let d=Connection::open(&path).unwrap();fixture(&d);
            seed(&d,1);
            snapshot(&d,&t,200);
            assert_eq!(row(&d,&t,0)["state"],"historical_stage_sealed");
        }
        {
            let d=Connection::open(&path).unwrap();
            init(&d).unwrap();
            assert_eq!(row(&d,&t,0)["state"],"historical_stage_sealed");
            let mut other=t.clone();other.login+=1;
            assert!(report(&d,&other,"demo").unwrap()["rows"].as_array().unwrap().is_empty());
            other=t.clone();other.demo=false;other.account_trade_mode=Some(2);
            assert_eq!(report(&d,&other,"live").unwrap()["status"],
                "unverified_demo_account");
            let before=row(&d,&t,0);
            snapshot(&d,&t,300);
            assert_eq!(row(&d,&t,0),before,
                "Repeated snapshots must not rewrite immutable broker evidence");
        }
        let _=std::fs::remove_file(path);
    }
}
