//! Durable TP-stage reservation ledger: the first crash-safe write after
//! a validated Core economic command draft, NOT a broker_commands producer.
//!
//! A reservation preserves the exact operator-selected account, immutable
//! TP plan and economic fields across restart. Duplicate ticks and changed
//! payloads never overwrite a previously reserved stage. An actual broker
//! enqueue/authorization will require a separate transaction and new checks.
use anyhow::{ensure,Result};
use braintrade_domain::Telemetry;
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};

fn context(t:&Telemetry)->String{
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
pub fn init(db:&Connection)->Result<()>{
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS partial_exit_stage_reservations(
            context TEXT NOT NULL,
            target TEXT NOT NULL CHECK(target IN ('demo','live')),
            position_identifier INTEGER NOT NULL CHECK(position_identifier>0),
            stage INTEGER NOT NULL CHECK(stage IN (1,2,3)),
            plan_id TEXT NOT NULL,
            physical_ticket INTEGER NOT NULL CHECK(physical_ticket>0),
            source_quote_ms INTEGER NOT NULL CHECK(source_quote_ms>0),
            prepared_ms INTEGER NOT NULL CHECK(prepared_ms>0),
            payload TEXT NOT NULL,
            state TEXT NOT NULL CHECK(state='reserved_not_dispatched'),
            PRIMARY KEY(context,target,position_identifier,stage),
            UNIQUE(context,target,plan_id)
        );
        CREATE INDEX IF NOT EXISTS tp_reservations_recent
            ON partial_exit_stage_reservations(context,target,prepared_ms DESC);"
    )?;
    Ok(())
}
fn equivalent_economic(saved:&Value,proposed:&Value)->bool{
    if saved["schema"]!="tp-stage-broker-command-shape-v1"
        ||proposed["schema"]!="tp-stage-broker-command-shape-v1"
        ||saved.get("id").is_some()||proposed.get("id").is_some()
        ||saved.get("state").is_some()||proposed.get("state").is_some()
    {return false}
    // Ticks and the provisional expiry vary; the actual economic decision
    // cannot. A revised broker binding/strategy/risk setting requires review.
    const FIELDS:&[&str]=&[
        "context","target","login","server","currency","symbol",
        "brain","magic","kind","position","lots","revision",
        "deviation_points","manual_close","manual_open","demo_flexible",
        "risk_master_enabled","tp_stage_plan_id","tp_stage",
        "expected_position_identifier","expected_position_side",
        "expected_position_volume","expected_position_entry",
        "expected_broker_tp","tp_target_price","max_quote_age_ms"
    ];
    FIELDS.iter().all(|field|saved.get(*field).is_some()
        &&saved.get(*field)==proposed.get(*field))
}
fn conflicting_close(db:&Connection,ctx:&str,physical_ticket:u64)->Result<bool>{
    // A malformed legacy close row is a conflicting claim until audited.
    let count:i64=db.query_row(
        "SELECT COUNT(*) FROM broker_commands WHERE context=?1 AND kind='close'
         AND CASE
          WHEN NOT json_valid(payload) THEN 1
          WHEN COALESCE(json_type(payload,'$.position'),'missing')!='integer' THEN 1
          WHEN CAST(json_extract(payload,'$.position') AS INTEGER)=?2 THEN 1
          ELSE 0 END=1",
        params![ctx,physical_ticket],|r|r.get(0))?;
    Ok(count!=0)
}
/// Record one eligible economic intent per immutable stage. It cannot send
/// and will never re-arm an older, altered quote after an interruption.
pub fn prepare(db:&Connection,t:&Telemetry,target:&str,now:u64,
    revision:u64,deviation:u32,risk_master:bool)->Result<Value>
{
    ensure!(matches!((target,t.demo,t.account_trade_mode),
        ("demo",true,Some(0))|("live",false,Some(2))),
        "Selected account and MT5 trade mode do not match");
    let candidates=crate::partial_exit_stage_producer::report(
        db,t,target,now,revision,deviation,risk_master)?;
    if candidates["status"]!="broker_command_shapes_not_queued"{
        return Ok(json!({"prepared":0,"idempotent":0,"conflicting":0,
            "status":"no_current_broker_validated_candidate",
            "broker_commands_created":0}));
    }
    let empty=Vec::new();
    let rows=candidates["rows"].as_array().unwrap_or(&empty);
    let key=context(t);
    let mut inserted=0_u64;
    let mut repeated=0_u64;
    let mut rejected=0_u64;
    // A single SQLite transaction makes both the plan uniqueness claim
    // and its immutable candidate bytes durable before publishing success.
    let tx=db.unchecked_transaction()?;
    for row in rows {
        let candidate=&row["broker_command_draft"];
        let Some(pid)=row["position_identifier"].as_u64().filter(|n|*n>0)else{continue};
        let Some(stage)=row["stage"].as_u64().filter(|n|(1..=3).contains(n))else{continue};
        let Some(plan)=row["plan_id"].as_str()else{continue};
        let Some(ticket)=candidate["position"].as_u64().filter(|n|*n>0)else{continue};
        let Some(source)=candidate["price_source_quote_ms"].as_u64()
            .filter(|n|*n>0&&*n<=now.saturating_add(2_000))else{continue};
        if candidate["context"]!=key||candidate["target"]!=target
            ||candidate["login"].as_u64()!=Some(t.login)
            ||candidate["expected_position_identifier"].as_u64()!=Some(pid)
            ||candidate["tp_stage"].as_u64()!=Some(stage)
            ||candidate["tp_stage_plan_id"]!=plan
            ||candidate["created_ms"].as_u64()!=Some(now)
            ||candidate.get("id").is_some()||candidate.get("state").is_some()
            ||row["orders_enabled"]!=false||row["broker_command_id"]!=Value::Null
            ||conflicting_close(&tx,&key,ticket)?
        {rejected+=1;continue}
        let saved:Option<(String,String,String)>=tx.query_row(
            "SELECT plan_id,payload,state FROM partial_exit_stage_reservations
             WHERE context=?1 AND target=?2 AND position_identifier=?3 AND stage=?4",
            params![key,target,pid,stage],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))
        ).optional()?;
        if let Some((original,raw,state))=saved{
            let previous=serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
            if state=="reserved_not_dispatched"&&original==plan
                &&equivalent_economic(&previous,candidate){
                repeated+=1;
            }else{rejected+=1}
            continue;
        }
        // A conflicting plan ID from a second physical position is not a
        // newer request; the uniqueness constraint permanently wins.
        let affected=tx.execute(
            "INSERT OR IGNORE INTO partial_exit_stage_reservations(
              context,target,position_identifier,stage,plan_id,physical_ticket,
              source_quote_ms,prepared_ms,payload,state)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'reserved_not_dispatched')",
            params![key,target,pid,stage,plan,ticket,source,now,candidate.to_string()]
        )?;
        if affected==1{inserted+=1}else{rejected+=1}
    }
    tx.commit()?;
    Ok(json!({"status":"reserved_only_no_gateway_dispatch",
        "prepared":inserted,"idempotent":repeated,"conflicting":rejected,
        "broker_commands_created":0,"orders_enabled":false}))
}
/// These entries are immutable historical intents; never say a saved row
/// remains approved to send based only on a new quote or a previous snapshot.
pub fn report(db:&Connection,t:&Telemetry,target:&str)->Result<Value>{
    let empty=||json!({"schema":"tp-stage-reservations-v1",
        "status":"unverified_selected_account","rows":[],
        "read_only":true,"orders_enabled":false,"broker_commands_created":0});
    if !matches!((target,t.demo,t.account_trade_mode),
        ("demo",true,Some(0))|("live",false,Some(2)))
        ||!t.connected||t.login==0||!t.validate(){return Ok(empty())}
    let key=context(t);
    let mut stmt=db.prepare(
        "SELECT position_identifier,stage,plan_id,physical_ticket,
                source_quote_ms,prepared_ms,payload,state
         FROM partial_exit_stage_reservations
         WHERE context=?1 AND target=?2 ORDER BY prepared_ms DESC LIMIT 50")?;
    let tuples=stmt.query_map(params![key,target],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,u32>(1)?,r.get::<_,String>(2)?,
        r.get::<_,u64>(3)?,r.get::<_,u64>(4)?,r.get::<_,u64>(5)?,
        r.get::<_,String>(6)?,r.get::<_,String>(7)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let rows=tuples.into_iter().map(|(pid,stage,plan,ticket,source,at,raw,state)|{
        let original=serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
        let intact=state=="reserved_not_dispatched"
            &&original["schema"]=="tp-stage-broker-command-shape-v1"
            &&original["context"]==key&&original["target"]==target
            &&original["position"].as_u64()==Some(ticket)
            &&original["expected_position_identifier"].as_u64()==Some(pid)
            &&original["tp_stage"].as_u64()==Some(stage as u64)
            &&original["tp_stage_plan_id"]==plan
            &&original["price_source_quote_ms"].as_u64()==Some(source)
            &&original["created_ms"].as_u64()==Some(at)
            &&original["kind"]=="close"
            &&original.get("id").is_none()&&original.get("state").is_none();
        json!({"position_identifier":pid,"stage":stage,"plan_id":plan,
            "physical_ticket":ticket,"source_quote_ms":source,"prepared_ms":at,
            "state":if intact{"reserved_only_no_send"}else{"invalid_reservation_requires_review"},
            "lots":if intact{original["lots"].clone()}else{Value::Null},
            "target_price":if intact{original["tp_target_price"].clone()}else{Value::Null},
            "read_only":true,"orders_enabled":false,"broker_command_id":Value::Null})
    }).collect::<Vec<_>>();
    Ok(json!({"schema":"tp-stage-reservations-v1",
        "status":"durable_stage_intents_not_dispatched",
        "context":key,"target":target,"rows":rows,
        "read_only":true,"orders_enabled":false,
        "broker_commands_created":0}))
}
