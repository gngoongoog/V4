//! Durable Core broker-command claim for TP1, HELD and never Gateway-dispatched.
//!
//! This is the first atomic broker_commands outbox boundary. Only a separate,
//! broker-tested release transition may change 'stage_held' to 'queued'.
//! The existing Gateway GET excludes this state and Core final authorization
//! requires 'dispatched'; previous stage markers stay blocked by Core.
use anyhow::Result;
use braintrade_domain::Telemetry;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

fn context(t:&Telemetry)->String {
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
pub fn init(db:&Connection)->Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS partial_exit_tp1_held(
          context TEXT NOT NULL,
          target TEXT NOT NULL CHECK(target IN ('demo','live')),
          position_identifier INTEGER NOT NULL CHECK(position_identifier>0),
          physical_ticket INTEGER NOT NULL CHECK(physical_ticket>0),
          plan_id TEXT NOT NULL UNIQUE,
          command_id TEXT NOT NULL UNIQUE,
          created_ms INTEGER NOT NULL CHECK(created_ms>0),
          immutable_command TEXT NOT NULL,
          state TEXT NOT NULL CHECK(state='held_never_dispatched'),
          PRIMARY KEY(context,target,position_identifier)
        );
        CREATE INDEX IF NOT EXISTS tp1_held_recent
          ON partial_exit_tp1_held(context,target,created_ms DESC);")?;
    Ok(())
}
fn same_command(saved:&str,expected:&Value)->bool {
    serde_json::from_str::<Value>(saved).ok().as_ref()==Some(expected)
}
/// This transaction either seals both the primary broker_command and its
/// account/plan uniqueness claim, or none. It NEVER yields a dispatchable
/// command. Existing immutable rows always win against later price ticks.
fn claim(db:&Connection,key:&str,target:&str,pid:u64,ticket:u64,
    plan:&str,draft:&Value,now:u64)->Result<&'static str>{
    if !matches!(target,"demo"|"live")||pid==0||ticket==0||now==0
        ||plan.len()!=40||!plan.starts_with("tp-plan-")
        ||!plan[8..].bytes().all(|c|c.is_ascii_hexdigit()&&!c.is_ascii_uppercase())
        ||draft["schema"]!="tp-stage-broker-command-shape-v1"
        ||draft["context"]!=key||draft["target"]!=target
        ||draft["kind"]!="close"||draft["tp_stage"]!=1
        ||draft["tp_stage_plan_id"]!=plan||draft["position"].as_u64()!=Some(ticket)
        ||draft["expected_position_identifier"].as_u64()!=Some(pid)
        ||draft["created_ms"].as_u64()!=Some(now)
        ||draft["expires_ms"].as_u64().is_none_or(|v|v<=now||v>now.saturating_add(3_000))
        ||draft["price_source_quote_ms"].as_u64().is_none_or(|v|
            v==0||v>now.saturating_add(2_000)||now.saturating_sub(v)>3_000)
        ||draft.get("id").is_some()||draft.get("state").is_some()
        ||draft["manual_close"]!=false||draft["manual_open"]!=false
        ||draft["lots"].as_f64().is_none_or(|v|!v.is_finite()||v<=0.)
        ||draft["revision"].as_u64().is_none_or(|v|v==0)
    {return Ok("invalid")}
    let tx=db.unchecked_transaction()?;
    let old:Option<(String,String,String,String)>=tx.query_row(
        "SELECT plan_id,command_id,immutable_command,state
         FROM partial_exit_tp1_held
         WHERE context=?1 AND target=?2 AND position_identifier=?3",
        params![key,target,pid],|r|Ok((
            r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?
        ))
    ).optional()?;
    if let Some((old_plan,id,raw,state))=old {
        let actual:Option<(String,String)>=tx.query_row(
            "SELECT payload,state FROM broker_commands WHERE id=?1",
            [&id],|r|Ok((r.get(0)?,r.get(1)?))
        ).optional()?;
        let valid=actual.is_some_and(|(payload,broker_state)|{
            let expected=serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
            old_plan==plan&&state=="held_never_dispatched"
                &&broker_state=="stage_held"
                &&expected["id"]==id
                &&expected["tp_stage_plan_id"]==plan
                &&same_command(&payload,&expected)
                &&same_command(&raw,&expected)
                &&expected.as_object().is_some_and(|obj|{
                    let mut plain=obj.clone();
                    plain.remove("id");
                    Value::Object(plain)==*draft
                })
        });
        return Ok(if valid{"idempotent"}else{"conflict"});
    }
    // Every prior close for this exact physical ticket (including a legacy
    // unparseable close) blocks creation. Prior MT5 sends are not erased.
    let existing:i64=tx.query_row(
        "SELECT COUNT(*) FROM broker_commands
         WHERE context=?1 AND kind='close'
         AND CASE WHEN NOT json_valid(payload) THEN 1
          WHEN COALESCE(json_type(payload,'$.position'),'missing')!='integer' THEN 1
          WHEN CAST(json_extract(payload,'$.position') AS INTEGER)=?2 THEN 1
          ELSE 0 END=1",
        params![key,ticket],|r|r.get(0))?;
    if existing>0{return Ok("conflict")}
    let id=uuid::Uuid::new_v4().simple().to_string();
    let mut payload=draft.clone();
    payload["id"]=json!(id);
    // No default queue state: this row cannot be selected by the Gateway.
    tx.execute(
        "INSERT INTO broker_commands(
          id,context,brain,kind,created_ms,state,payload)
         VALUES(?1,?2,?3,'close',?4,'stage_held',?5)",
        params![id,key,draft["brain"].as_str().unwrap_or("system"),now,
            payload.to_string()])?;
    tx.execute(
        "INSERT INTO partial_exit_tp1_held(
          context,target,position_identifier,physical_ticket,plan_id,
          command_id,created_ms,immutable_command,state)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'held_never_dispatched')",
        params![key,target,pid,ticket,plan,id,now,payload.to_string()])?;
    tx.execute(
        "INSERT INTO events(timestamp_ms,kind,detail)
         VALUES(?1,'tp1_held_not_broker_dispatched',?2)",
        params![now,id])?;
    tx.commit()?;
    Ok("held")
}
/// The two independent operator records are essential. A historical plan
/// preparation consent is NOT a trading approval; neither alone is enough.
/// The whole physical position and source quote still must match the Core
/// draft and the same fresh TP1 seal before a held outbox claim is created.
pub fn prepare(db:&Connection,t:&Telemetry,target:&str,now:u64,
    revision:u64,deviation:u32,risk_master:bool)->Result<Value>{
    let empty=||json!({"status":"no_verified_trade_and_stage_consent",
        "held":0,"idempotent":0,"conflicting":0,
        "orders_dispatched":0,"gateway_sends":0});
    if !t.connected||t.login==0||t.margin_mode!=Some(2)||!t.validate()
        ||!matches!((target,t.demo,t.account_trade_mode),
            ("demo",true,Some(0))|("live",false,Some(2)))
        ||revision==0||!t.quote_is_fresh(now,3_000)
    {return Ok(empty())}
    let key=context(t);
    // Cheap no-approval shortcut keeps normal trading and the Sniper fast
    // path independent of the expensive experimental TP lineage engine.
    let exists:i64=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM partial_exit_tp1_trade_approvals
         WHERE context=?1 AND target=?2 AND enabled=1
         AND settings_revision=?3 AND expires_ms>?4)",
        params![key,target,revision,now],|r|r.get(0))?;
    if exists!=1{return Ok(empty())}
    let trading=crate::partial_exit_trade_approval::report(
        db,t,target,now,revision)?;
    let prepared=crate::partial_exit_stage_outbox::report(
        db,t,target,now,revision)?;
    if trading["status"]!="account_scoped_trade_preference_not_execution"
        ||prepared["status"]!="account_scoped_prepared_not_sent"
        ||trading["context"]!=key||prepared["context"]!=key
        ||trading["target"]!=target||prepared["target"]!=target
    {return Ok(empty())}
    let drafts=crate::partial_exit_stage_producer::report(
        db,t,target,now,revision,deviation,risk_master)?;
    if drafts["status"]!="broker_command_shapes_not_queued"
        ||drafts["context"]!=key||drafts["target"]!=target
    {return Ok(empty())}
    let trade_rows=trading["rows"].as_array().cloned().unwrap_or_default();
    let sealed_rows=prepared["rows"].as_array().cloned().unwrap_or_default();
    let mut held=0;let mut repeated=0;let mut conflict=0;
    for row in drafts["rows"].as_array().into_iter().flatten(){
        if row["stage"]!=1||row["orders_enabled"]!=false
            ||!row["broker_command_id"].is_null(){continue}
        let Some(pid)=row["position_identifier"].as_u64().filter(|v|*v>0)else{continue};
        let Some(plan)=row["plan_id"].as_str()else{continue};
        let draft=&row["broker_command_draft"];
        let Some(ticket)=draft["position"].as_u64().filter(|v|*v>0)else{continue};
        let approved=trade_rows.iter().any(|r|
            r["position_identifier"].as_u64()==Some(pid)
            &&r["physical_ticket"].as_u64()==Some(ticket)
            &&r["trade_preference_current"]==true
            &&r["state"]=="trade_preference_current_sender_locked"
            &&r["plan_ids"][0]==plan);
        let sealed=sealed_rows.iter().any(|r|
            r["position_identifier"].as_u64()==Some(pid)
            &&r["physical_ticket"].as_u64()==Some(ticket)
            &&r["plan_id"]==plan
            &&r["state"]=="sealed_unarmed_not_dispatched"
            &&r["broker_command_id"].is_null());
        if !approved||!sealed||draft["revision"].as_u64()!=Some(revision){
            conflict+=1;continue
        }
        // One transaction does the outbox+claim. A TP1 held record will
        // never be eligible for GET gateway or authorize without a later,
        // independently audited state transition.
        match claim(db,&key,target,pid,ticket,plan,draft,now)?{
            "held"=>held+=1,
            "idempotent"=>repeated+=1,
            _=>conflict+=1
        }
    }
    Ok(json!({"status":"tp1_broker_outbox_held_no_dispatch",
        "held":held,"idempotent":repeated,"conflicting":conflict,
        "orders_dispatched":0,"gateway_sends":0,
        "note":"Real broker_commands stage_held rows exist, but are not queued or authorized."}))
}
pub fn report(db:&Connection,t:&Telemetry,target:&str,now:u64)->Result<Value>{
    let mut result=json!({"schema":"tp1-core-held-outbox-v1",
        "status":"bound_account_required","rows":[],
        "read_only":true,"orders_dispatched":0,"gateway_sends":0});
    if !t.connected||t.login==0||!t.validate()
        ||!matches!((target,t.demo,t.account_trade_mode),
            ("demo",true,Some(0))|("live",false,Some(2)))
    {return Ok(result)}
    let key=context(t);
    let mut stmt=db.prepare(
        "SELECT position_identifier,physical_ticket,plan_id,command_id,
            created_ms,immutable_command,state
         FROM partial_exit_tp1_held
         WHERE context=?1 AND target=?2
         ORDER BY created_ms DESC LIMIT 50")?;
    let rows=stmt.query_map(params![key,target],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,
        r.get::<_,String>(2)?,r.get::<_,String>(3)?,
        r.get::<_,u64>(4)?,r.get::<_,String>(5)?,
        r.get::<_,String>(6)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    result["status"]=json!("account_scoped_held_never_sent");
    result["context"]=json!(key);result["target"]=json!(target);
    result["rows"]=json!(rows.into_iter().map(|(pid,ticket,plan,id,at,raw,state)|{
        let broker:Option<(String,String)>=db.query_row(
            "SELECT payload,state FROM broker_commands WHERE id=?1",
            [&id],|r|Ok((r.get(0)?,r.get(1)?))
        ).optional().unwrap_or(None);
        let intact=broker.is_some_and(|(payload,broker_state)|
            broker_state=="stage_held"
            &&state=="held_never_dispatched"
            &&same_command(&payload,&serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null)));
        json!({"position_identifier":pid,"physical_ticket":ticket,
            "plan_id":plan,"held_core_command_id":id,
            "created_ms":at,"state":if !intact{"held_outbox_corrupted_needs_review"}
                else if now>=at&&now-at<=3_000{"held_unreleased_no_gateway"}
                else{"held_expired_never_dispatched"},
            "orders_dispatched":0,"gateway_sends":0,
            "read_only":true})
    }).collect::<Vec<_>>());
    Ok(result)
}
#[cfg(test)]
mod tests{
    use super::*;
    fn setup(db:&Connection){
        db.execute_batch("CREATE TABLE broker_commands(
          id TEXT PRIMARY KEY,context TEXT,brain TEXT,kind TEXT,
          created_ms INTEGER,state TEXT,payload TEXT,result TEXT DEFAULT '{}');
          CREATE TABLE events(id INTEGER PRIMARY KEY,
            timestamp_ms INTEGER,kind TEXT,detail TEXT);").unwrap();
        init(db).unwrap();
    }
    fn candidate()->Value{
        json!({"schema":"tp-stage-broker-command-shape-v1",
            "context":"ctx","target":"demo","login":17,"server":"D",
            "currency":"USD","symbol":"XAUUSD","brain":"system",
            "magic":410601,"kind":"close","position":87,"lots":0.05,
            "revision":5,"manual_close":false,"manual_open":false,
            "created_ms":1550,"expires_ms":4550,"tp_stage":1,
            "tp_stage_plan_id":format!("tp-plan-{}","a".repeat(32)),
            "expected_position_identifier":907,"price_source_quote_ms":1500})
    }
    #[test]
    fn crash_atomic_held_command_cannot_dispatch_and_cannot_duplicate(){
        let db=Connection::open_in_memory().unwrap();setup(&db);
        let c=candidate();let plan=c["tp_stage_plan_id"].as_str().unwrap();
        assert_eq!(claim(&db,"ctx","demo",907,87,plan,&c,1550).unwrap(),"held");
        assert_eq!(claim(&db,"ctx","demo",907,87,plan,&c,1550).unwrap(),"idempotent");
        let mut altered=c.clone();altered["lots"]=json!(0.08);
        assert_eq!(claim(&db,"ctx","demo",907,87,plan,&altered,1550).unwrap(),"conflict");
        assert_eq!(claim(&db,"ctx","live",907,87,plan,&c,1550).unwrap(),"invalid");
        let (state,stored):(String,String)=db.query_row(
            "SELECT state,payload FROM broker_commands",[],
            |r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(state,"stage_held");
        assert_eq!(serde_json::from_str::<Value>(&stored).unwrap()["kind"],"close");
        assert_eq!(db.query_row(
            "SELECT COUNT(*) FROM broker_commands WHERE state IN ('queued','dispatched')",
            [],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM events",[],
            |r|r.get::<_,i64>(0)).unwrap(),1);
    }
    #[test]
    fn tampered_or_old_unknown_close_blocks_tp1_claim(){
        let db=Connection::open_in_memory().unwrap();setup(&db);
        let c=candidate();let plan=c["tp_stage_plan_id"].as_str().unwrap();
        db.execute("INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload)
            VALUES('previous','ctx','system','close',100,'unknown','{\"position\":87}')",
            []).unwrap();
        assert_eq!(claim(&db,"ctx","demo",907,87,plan,&c,1550).unwrap(),"conflict");
        db.execute("DELETE FROM broker_commands",[]).unwrap();
        assert_eq!(claim(&db,"ctx","demo",907,87,plan,&c,1550).unwrap(),"held");
        db.execute("UPDATE broker_commands SET state='dispatched'",[]).unwrap();
        assert_eq!(claim(&db,"ctx","demo",907,87,plan,&c,1550).unwrap(),"conflict");
    }
    #[test]
    fn durable_held_claim_survives_restart_without_broker_send(){
        let file=std::env::temp_dir().join(format!(
            "bt-tp1-held-{}.sqlite",uuid::Uuid::new_v4().simple()));
        let c=candidate();let plan=c["tp_stage_plan_id"].as_str().unwrap().to_string();
        {
            let db=Connection::open(&file).unwrap();setup(&db);
            assert_eq!(claim(&db,"ctx","demo",907,87,&plan,&c,1550).unwrap(),"held");
        }
        {
            let db=Connection::open(&file).unwrap();init(&db).unwrap();
            assert_eq!(claim(&db,"ctx","demo",907,87,&plan,&c,1550).unwrap(),"idempotent");
            let n:i64=db.query_row(
                "SELECT COUNT(*) FROM broker_commands WHERE state='stage_held'",
                [],|r|r.get(0)).unwrap();
            assert_eq!(n,1);
            let n:i64=db.query_row(
                "SELECT COUNT(*) FROM broker_commands WHERE state IN ('queued','dispatched')",
                [],|r|r.get(0)).unwrap();
            assert_eq!(n,0);
        }
        let _=std::fs::remove_file(file);
    }
}