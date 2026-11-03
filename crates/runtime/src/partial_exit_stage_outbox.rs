//! Account-bound TP1 Core command identity, sealed BEFORE any executable outbox.
//!
//! This table is deliberately NOT broker_commands: neither Gateway polling nor
//! final order authorization can read an entry from here. Older operator
//! consent is preparation_only and must never silently become live trading
//! permission. A separately reviewed trade approval + atomic promotion will
//! be required to create a real broker command.
use anyhow::Result;
use braintrade_domain::Telemetry;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

fn account_key(t:&Telemetry)->String {
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
pub fn init(db:&Connection)->Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS partial_exit_sealed_core_intents(
        context TEXT NOT NULL,
        target TEXT NOT NULL CHECK(target IN ('demo','live')),
        position_identifier INTEGER NOT NULL CHECK(position_identifier>0),
        stage INTEGER NOT NULL CHECK(stage=1),
        plan_id TEXT NOT NULL,
        physical_ticket INTEGER NOT NULL CHECK(physical_ticket>0),
        core_intent_id TEXT NOT NULL UNIQUE,
        sealed_ms INTEGER NOT NULL CHECK(sealed_ms>0),
        original_command TEXT NOT NULL,
        state TEXT NOT NULL CHECK(state='sealed_not_queued'),
        PRIMARY KEY(context,target,position_identifier,stage),
        UNIQUE(context,target,plan_id)
    );
    CREATE INDEX IF NOT EXISTS tp_sealed_core_recent
        ON partial_exit_sealed_core_intents(context,target,sealed_ms DESC);")?;
    Ok(())
}
fn valid_id(plan:&str)->bool {
    plan.len()==40&&plan.starts_with("tp-plan-")
        &&plan[8..].bytes().all(|c|c.is_ascii_hexdigit()&&!c.is_ascii_uppercase())
}
/// One SQLite transaction owns the stage identity and the audit event.
/// No broker_commands insert, dispatch state or Gateway send is possible.
fn seal_verified(db:&Connection,key:&str,target:&str,pid:u64,plan:&str,
    physical_ticket:u64,command:&Value,now:u64)->Result<&'static str>
{
    if !matches!(target,"demo"|"live")||pid==0||physical_ticket==0||now==0
        ||!valid_id(plan)||command["schema"]!="tp-stage-broker-command-shape-v1"
        ||command["context"]!=key||command["target"]!=target
        ||command["tp_stage"]!=1||command["kind"]!="close"
        ||command["expected_position_identifier"].as_u64()!=Some(pid)
        ||command["tp_stage_plan_id"]!=plan
        ||command["position"].as_u64()!=Some(physical_ticket)
        ||command["created_ms"].as_u64()!=Some(now)
        ||command["price_source_quote_ms"].as_u64().is_none_or(|s|
            s==0||s>now.saturating_add(2_000)||now.saturating_sub(s)>3_000)
        ||command["expires_ms"].as_u64().is_none_or(|end|
            end<=now||end>now.saturating_add(3_000))
        ||command.get("id").is_some()||command.get("state").is_some()
    {return Ok("invalid")}
    let tx=db.unchecked_transaction()?;
    let old:Option<(String,String,String)>=tx.query_row(
        "SELECT plan_id,original_command,state
         FROM partial_exit_sealed_core_intents
         WHERE context=?1 AND target=?2 AND position_identifier=?3 AND stage=1",
        params![key,target,pid],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))
    ).optional()?;
    if let Some((old_plan,original,state))=old {
        return Ok(if state=="sealed_not_queued"&&old_plan==plan
            &&serde_json::from_str::<Value>(&original).ok()==Some(command.clone()){
            "idempotent"
        }else{"conflict"})
    }
    // A previously issued close of this physical ticket is never erased or
    // reinterpreted as a new TP1, regardless of broker journal state.
    let conflicting:i64=tx.query_row(
        "SELECT COUNT(*) FROM broker_commands WHERE context=?1 AND kind='close'
         AND CASE WHEN NOT json_valid(payload) THEN 1
          WHEN COALESCE(json_type(payload,'$.position'),'missing')!='integer' THEN 1
          WHEN CAST(json_extract(payload,'$.position') AS INTEGER)=?2 THEN 1
          ELSE 0 END=1",
        params![key,physical_ticket],|r|r.get(0))?;
    if conflicting!=0{return Ok("conflict")}
    let opaque=format!("tp-core-sealed-{}",uuid::Uuid::new_v4().simple());
    let affected=tx.execute(
        "INSERT OR IGNORE INTO partial_exit_sealed_core_intents(
         context,target,position_identifier,stage,plan_id,physical_ticket,
         core_intent_id,sealed_ms,original_command,state)
         VALUES(?1,?2,?3,1,?4,?5,?6,?7,?8,'sealed_not_queued')",
        params![key,target,pid,plan,physical_ticket,opaque,now,command.to_string()])?;
    if affected!=1{return Ok("conflict")}
    tx.execute(
        "INSERT INTO events(timestamp_ms,kind,detail)
         VALUES(?1,'tp1_core_intent_sealed_not_sent',?2)",
        params![now,opaque])?;
    tx.commit()?;
    Ok("sealed")
}
/// Only fresh, prevalidated Core producer candidates backed by the exact
/// immutable reservation AND historical preparatory approval may be sealed.
/// Deliberately no execution approval, order ID or broker-command write.
pub fn seal_tp1(db:&Connection,t:&Telemetry,target:&str,now:u64,
    revision:u64,deviation:u32,risk_master:bool)->Result<Value>
{
    let empty=||json!({"status":"unverified_no_stage_sealed",
        "sealed":0,"idempotent":0,"conflicting":0,
        "orders_enabled":false,"broker_commands_created":0});
    if !t.connected||t.login==0||t.margin_mode!=Some(2)||!t.validate()
        ||!matches!((target,t.demo,t.account_trade_mode),
            ("demo",true,Some(0))|("live",false,Some(2)))
        ||revision==0||!t.quote_is_fresh(now,3_000)
    {return Ok(empty())}
    let key=account_key(t);
    // On most pulses there is no explicit operator preparation approval.
    // Avoid re-running the expensive multi-table producer in that case.
    // This is a read-only indexed account/revision check, not permission to
    // dispatch: the full current lineage is validated below regardless.
    let any_approval:i64=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM partial_exit_stage_consents
         WHERE context=?1 AND target=?2 AND enabled=1
         AND purpose='preparation_only' AND approved_revision=?3)",
        params![key,target,revision],|r|r.get(0))?;
    if any_approval!=1{return Ok(empty())}
    let candidates=crate::partial_exit_stage_producer::report(
        db,t,target,now,revision,deviation,risk_master)?;
    if candidates["status"]!="broker_command_shapes_not_queued"
        ||candidates["context"]!=key||candidates["target"]!=target
    {return Ok(empty())}
    let approved=crate::partial_exit_stage_consent::report_for_revision(
        db,t,target,revision)?;
    if approved["context"]!=key||approved["target"]!=target
        ||approved["status"]!="account_scoped_operator_consent_not_execution"
    {return Ok(empty())}
    let consent_rows=approved["rows"].as_array().cloned().unwrap_or_default();
    let mut sealed=0;let mut repeated=0;let mut conflict=0;
    for row in candidates["rows"].as_array().into_iter().flatten() {
        if row["stage"]!=1||row["orders_enabled"]!=false
            ||!row["broker_command_id"].is_null(){continue}
        let Some(pid)=row["position_identifier"].as_u64().filter(|x|*x>0)
            else{continue};
        let Some(plan)=row["plan_id"].as_str().filter(|p|valid_id(p))
            else{continue};
        let candidate=&row["broker_command_draft"];
        let Some(ticket)=candidate["position"].as_u64().filter(|x|*x>0)
            else{continue};
        if candidate["revision"].as_u64()!=Some(revision)
            ||candidate["tp_stage"]!=1
            ||candidate["tp_stage_plan_id"]!=plan
            ||candidate["expected_position_identifier"].as_u64()!=Some(pid)
            ||candidate["created_ms"].as_u64()!=Some(now)
            ||!consent_rows.iter().any(|consent|
                consent["position_identifier"].as_u64()==Some(pid)
                    &&consent["position_ticket"].as_u64()==Some(ticket)
                    &&consent["operator_approved"]==true
                    &&consent["approved_revision"].as_u64()==Some(revision)
                    &&consent["purpose"]=="preparation_only"
                    &&consent["state"]=="operator_approved_not_dispatched"
                    &&consent["plan_ids"].as_array().is_some_and(|ids|
                        ids.first()==Some(&json!(plan))))
        {conflict+=1;continue}
        // Do not trust candidate provenance alone. The first reservation
        // must match the exact same command bytes and source quote.
        let reservation:Option<(String,u64,u64,String,String)>=db.query_row(
            "SELECT plan_id,physical_ticket,source_quote_ms,payload,state
             FROM partial_exit_stage_reservations
             WHERE context=?1 AND target=?2 AND position_identifier=?3 AND stage=1",
            params![key,target,pid],|r|Ok((
                r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?
            ))
        ).optional()?;
        let matching=reservation.is_some_and(|(old_plan,old_ticket,old_source,raw,state)|
            state=="reserved_not_dispatched"&&old_plan==plan
                &&old_ticket==ticket
                &&candidate["price_source_quote_ms"].as_u64()==Some(old_source)
                &&serde_json::from_str::<Value>(&raw).ok()==Some(candidate.clone()));
        if !matching {conflict+=1;continue}
        match seal_verified(db,&key,target,pid,plan,ticket,candidate,now)? {
            "sealed"=>sealed+=1,
            "idempotent"=>repeated+=1,
            _=>conflict+=1
        }
    }
    Ok(json!({"status":"tp1_core_identities_sealed_not_executable",
        "sealed":sealed,"idempotent":repeated,"conflicting":conflict,
        "orders_enabled":false,"broker_commands_created":0,
        "note":"Research preparation only. Separate explicit trade approval and broker outbox promotion not implemented."}))
}
/// Historical seal is visible but NEVER an effective broker instruction.
/// Price-crossing age expires independently of the immutable audit record.
pub fn report(db:&Connection,t:&Telemetry,target:&str,now:u64,revision:u64)->Result<Value> {
    let mut result=json!({"schema":"tp-stage-sealed-core-intents-v1",
        "status":"unverified_account","rows":[],"read_only":true,
        "orders_enabled":false,"broker_commands_created":0});
    if !t.connected||t.login==0||!t.validate()
        ||!matches!((target,t.demo,t.account_trade_mode),
            ("demo",true,Some(0))|("live",false,Some(2)))
    {return Ok(result)}
    let key=account_key(t);
    let mut stmt=db.prepare("SELECT position_identifier,plan_id,physical_ticket,
          core_intent_id,sealed_ms,original_command,state
          FROM partial_exit_sealed_core_intents
          WHERE context=?1 AND target=?2 ORDER BY sealed_ms DESC LIMIT 50")?;
    let rows=stmt.query_map(params![key,target],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,String>(1)?,
        r.get::<_,u64>(2)?,r.get::<_,String>(3)?,
        r.get::<_,u64>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let rows=rows.into_iter().map(|(pid,plan,ticket,id,at,raw,state)|{
        let parsed=serde_json::from_str::<Value>(&raw).unwrap_or(Value::Null);
        let intact=state=="sealed_not_queued"&&valid_id(&plan)
            &&parsed["schema"]=="tp-stage-broker-command-shape-v1"
            &&parsed["context"]==key&&parsed["target"]==target
            &&parsed["tp_stage"]==1&&parsed["tp_stage_plan_id"]==plan
            &&parsed["expected_position_identifier"].as_u64()==Some(pid)
            &&parsed["position"].as_u64()==Some(ticket)
            &&parsed["created_ms"].as_u64()==Some(at)
            &&parsed.get("id").is_none();
        // A sealed command is never currently meaningful if its original
        // position vanished, account settings revision changed or today's
        // MT5 quote is stale. The audit row survives but cannot be rearmed.
        let positions=t.positions.iter().filter(|p|
            p.identifier==pid||p.ticket==ticket).collect::<Vec<_>>();
        let position_current=positions.len()==1&&{
            let p=positions[0];
            let step=t.volume_step.unwrap_or(f64::NAN);
            let tick=t.tick_size.unwrap_or(f64::NAN);
            p.identifier==pid&&p.ticket==ticket&&p.symbol==t.symbol
                &&parsed["magic"].as_u64()==Some(p.magic)
                &&parsed["expected_position_side"]==p.side
                &&step.is_finite()&&step>0.&&tick.is_finite()&&tick>0.
                &&parsed["expected_position_volume"].as_f64().is_some_and(|v|
                    v.is_finite()&&(v-p.volume).abs()<=step*1e-6)
                &&parsed["expected_position_entry"].as_f64().is_some_and(|v|
                    v.is_finite()&&(v-p.price_open).abs()<=tick*1e-6)
                &&parsed["expected_broker_tp"].as_f64().is_some_and(|v|
                    v.is_finite()&&(v-p.tp).abs()<=tick*1e-6)
        };
        let recent=intact&&revision>0&&parsed["revision"].as_u64()==Some(revision)
            &&position_current&&t.quote_is_fresh(now,3_000)
            &&now>=at&&now.saturating_sub(at)<=3_000;
        json!({"position_identifier":pid,"stage":1,"plan_id":plan,
            "physical_ticket":ticket,"core_sealed_id":id,"sealed_ms":at,
            "state":if !intact{"corrupted_seal_needs_review"}
                else if recent{"sealed_unarmed_not_dispatched"}
                else{"historical_seal_expired_no_send"},
            "orders_enabled":false,"trade_approval":false,
            "broker_command_id":Value::Null,
            "gateway_send_id":Value::Null,"broker_order_ticket":Value::Null,
            "read_only":true})
    }).collect::<Vec<_>>();
    result["status"]=json!("account_scoped_prepared_not_sent");
    result["context"]=json!(key);result["target"]=json!(target);
    result["rows"]=json!(rows);
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn setup(db:&Connection){
        db.execute_batch("CREATE TABLE events(
            id INTEGER PRIMARY KEY,timestamp_ms INTEGER,kind TEXT,detail TEXT);
            CREATE TABLE broker_commands(id TEXT,context TEXT,brain TEXT,kind TEXT,
            created_ms INTEGER,state TEXT,payload TEXT,result TEXT);").unwrap();
        init(db).unwrap();
    }
    fn candidate()->Value {
        json!({"schema":"tp-stage-broker-command-shape-v1",
            "context":"ctx","target":"demo","position":87,
            "kind":"close","tp_stage":1,"tp_stage_plan_id":
                format!("tp-plan-{}","a".repeat(32)),
            "expected_position_identifier":907,"price_source_quote_ms":1500,
            "created_ms":1550,"expires_ms":4550,"lots":0.05,
            "expected_position_volume":0.1,"tp_target_price":2001.0})
    }
    #[test]
    fn tp1_claim_is_atomic_idempotent_and_does_not_create_broker_command(){
        let db=Connection::open_in_memory().unwrap();setup(&db);
        let c=candidate();
        let p=c["tp_stage_plan_id"].as_str().unwrap();
        assert_eq!(seal_verified(&db,"ctx","demo",907,p,87,&c,1550).unwrap(),"sealed");
        assert_eq!(seal_verified(&db,"ctx","demo",907,p,87,&c,1550).unwrap(),"idempotent");
        let mut altered=c.clone();altered["lots"]=json!(0.08);
        assert_eq!(seal_verified(&db,"ctx","demo",907,p,87,&altered,1550).unwrap(),"conflict");
        assert_eq!(seal_verified(&db,"ctx","live",907,p,87,&c,1550).unwrap(),"invalid");
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_commands",[],
            |r|r.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM events",[],
            |r|r.get::<_,i64>(0)).unwrap(),1);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM partial_exit_sealed_core_intents",[],
            |r|r.get::<_,i64>(0)).unwrap(),1);
    }
    #[test]
    fn readback_revocation_missing_position_and_expiry_never_imply_send(){
        use braintrade_domain::Position;
        let db=Connection::open_in_memory().unwrap();setup(&db);
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            login:17,server:"DemoA".into(),symbol:"XAUUSD".into(),
            currency:"USD".into(),margin_mode:Some(2),volume_step:Some(0.01),
            tick_size:Some(0.01),quote_time_ms:Some(1550),
            source_quote_time_ms:Some(1500),tick_age_ms:Some(0),
            positions:vec![Position{identifier:907,ticket:87,magic:410601,
                symbol:"XAUUSD".into(),side:"BUY".into(),volume:0.1,
                price_open:1999.5,tp:0.0,..Default::default()}],
            ..Default::default()};
        let key=account_key(&t);
        let mut c=candidate();
        c["context"]=json!(key);c["login"]=json!(17);
        c["revision"]=json!(5);c["magic"]=json!(410601);
        c["expected_position_side"]=json!("BUY");
        c["expected_position_entry"]=json!(1999.5);
        c["expected_broker_tp"]=json!(0.0);
        let plan=c["tp_stage_plan_id"].as_str().unwrap();
        assert_eq!(seal_verified(&db,&key,"demo",907,plan,87,&c,1550).unwrap(),"sealed");
        let fresh=report(&db,&t,"demo",1550,5).unwrap();
        assert_eq!(fresh["rows"][0]["state"],"sealed_unarmed_not_dispatched");
        assert_eq!(fresh["rows"][0]["broker_command_id"],Value::Null);
        assert_eq!(fresh["rows"][0]["trade_approval"],false);
        let wrong_revision=report(&db,&t,"demo",1550,6).unwrap();
        assert_eq!(wrong_revision["rows"][0]["state"],"historical_seal_expired_no_send");
        let mut missing=t.clone();missing.positions.clear();
        assert_eq!(report(&db,&missing,"demo",1550,5).unwrap()["rows"][0]["state"],
            "historical_seal_expired_no_send");
        assert_eq!(report(&db,&t,"demo",10_000,5).unwrap()["rows"][0]["state"],
            "historical_seal_expired_no_send");
        let mut foreign=t.clone();foreign.login+=1;
        assert!(report(&db,&foreign,"demo",1550,5).unwrap()["rows"].as_array().unwrap().is_empty());
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_commands",[],
            |r|r.get::<_,i64>(0)).unwrap(),0);
    }
    #[test]
    fn no_implicit_stage_sealing_without_preparatory_consent(){
        let db=Connection::open_in_memory().unwrap();setup(&db);
        crate::partial_exit_stage_consent::init(&db).unwrap();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            margin_mode:Some(2),login:17,server:"DemoA".into(),
            currency:"USD".into(),symbol:"XAUUSD".into(),
            source_quote_time_ms:Some(1500),quote_time_ms:Some(1550),
            tick_age_ms:Some(0),..Default::default()};
        let reply=seal_tp1(&db,&t,"demo",1550,5,20,true).unwrap();
        assert_eq!(reply["sealed"],0);
        assert_eq!(reply["orders_enabled"],false);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM partial_exit_sealed_core_intents",[],
            |r|r.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM broker_commands",[],
            |r|r.get::<_,i64>(0)).unwrap(),0);
    }
    #[test]
    fn foreign_collision_and_broker_close_prevent_stage_identity(){
        let db=Connection::open_in_memory().unwrap();setup(&db);
        let c=candidate();let plan=c["tp_stage_plan_id"].as_str().unwrap();
        db.execute("INSERT INTO broker_commands VALUES(
            'prior','ctx','system','close',100,'unknown',
            '{\"position\":87}','{}')",[]).unwrap();
        assert_eq!(seal_verified(&db,"ctx","demo",907,plan,87,&c,1550).unwrap(),"conflict");
        db.execute("DELETE FROM broker_commands",[]).unwrap();
        assert_eq!(seal_verified(&db,"ctx","demo",907,plan,87,&c,1550).unwrap(),"sealed");
        let mut foreign=c.clone();foreign["expected_position_identifier"]=json!(999);
        assert_eq!(seal_verified(&db,"ctx","demo",999,plan,87,&foreign,1550).unwrap(),"conflict");
        assert_eq!(db.query_row("SELECT COUNT(*) FROM partial_exit_sealed_core_intents",[],
            |r|r.get::<_,i64>(0)).unwrap(),1);
    }
    #[test]
    fn sealed_identity_survives_restart_without_reviving_expired_quote(){
        let path=std::env::temp_dir().join(format!("tp-seal-{}.db",uuid::Uuid::new_v4().simple()));
        let c=candidate();let p=c["tp_stage_plan_id"].as_str().unwrap().to_string();
        {
            let db=Connection::open(&path).unwrap();setup(&db);
            assert_eq!(seal_verified(&db,"ctx","demo",907,&p,87,&c,1550).unwrap(),"sealed");
        }
        {
            let db=Connection::open(&path).unwrap();init(&db).unwrap();
            assert_eq!(seal_verified(&db,"ctx","demo",907,&p,87,&c,1550).unwrap(),"idempotent");
            let count:i64=db.query_row("SELECT COUNT(*) FROM broker_commands",[],
                |r|r.get(0)).unwrap();
            assert_eq!(count,0);
            let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
                login:17,server:"Server".into(),symbol:"XAUUSD".into(),
                currency:"USD".into(),..Default::default()};
            assert_eq!(report(&db,&t,"live",10_000,5).unwrap()["status"],"unverified_account");
            assert!(report(&db,&t,"demo",10_000,5).unwrap()["rows"].as_array().unwrap().is_empty());
        }
        let _=std::fs::remove_file(path);
    }
}