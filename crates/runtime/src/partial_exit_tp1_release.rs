//! TP1 conditional one-shot held -> queued transition with durable release proof.
//!
//! Only the deliberately armed, fully validated MT5 position can be released.
//! No "preparation_only" or stored trade preference can cause a broker send
//! without a separate operator execution arm. Gateway and Core must re-check
//! the immutable release before dispatch AND at final authorization.
use anyhow::Result;
use braintrade_domain::{execution,Telemetry};
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};

fn key(t:&Telemetry)->String{
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
pub fn init(db:&Connection)->Result<()>{
    db.execute_batch("CREATE TABLE IF NOT EXISTS partial_exit_tp1_releases(
       command_id TEXT NOT NULL PRIMARY KEY,
       context TEXT NOT NULL,
       target TEXT NOT NULL CHECK(target IN ('demo','live')),
       position_identifier INTEGER NOT NULL CHECK(position_identifier>0),
       physical_ticket INTEGER NOT NULL CHECK(physical_ticket>0),
       plan_id TEXT NOT NULL,
       released_ms INTEGER NOT NULL CHECK(released_ms>0),
       immutable_command TEXT NOT NULL,
       state TEXT NOT NULL CHECK(state='released_once'),
       UNIQUE(context,target,position_identifier),
       UNIQUE(context,target,plan_id)
    );
    CREATE INDEX IF NOT EXISTS tp1_releases_context
       ON partial_exit_tp1_releases(context,target,released_ms DESC);")?;
    Ok(())
}
fn near(a:f64,b:f64,step:f64)->bool{
    a.is_finite()&&b.is_finite()&&step.is_finite()&&step>0.
        &&(a-b).abs()<=step*1e-6
}
fn snapshot_valid(t:&Telemetry,target:&str,pid:u64,ticket:u64,
    cmd:&Value,revision:u64,now:u64)->bool{
    if !t.connected||t.login==0||t.margin_mode!=Some(2)||!t.validate()
        ||!matches!((target,t.demo,t.account_trade_mode),
            ("demo",true,Some(0))|("live",false,Some(2)))
        ||!t.quote_is_fresh(now,3_000)
        ||cmd["schema"]!="tp-stage-broker-command-shape-v1"
        ||cmd["kind"]!="close"||cmd["tp_stage"]!=1
        ||cmd["context"]!=key(t)||cmd["target"]!=target
        ||cmd["login"].as_u64()!=Some(t.login)
        ||cmd["server"]!=t.server||cmd["currency"]!=t.currency
        ||cmd["symbol"]!=t.symbol
        ||cmd["manual_close"]!=false||cmd["manual_open"]!=false
        ||cmd["revision"].as_u64()!=Some(revision)
        ||cmd["expected_position_identifier"].as_u64()!=Some(pid)
        ||cmd["position"].as_u64()!=Some(ticket)
        ||cmd["expires_ms"].as_u64().is_none_or(|v|v<=now)
        ||cmd["created_ms"].as_u64().is_none_or(|v|v==0||v>now
            ||now.saturating_sub(v)>3_000)
        ||cmd["price_source_quote_ms"].as_u64().is_none_or(|v|
            v==0||v>now.saturating_add(2_000)
            ||now.saturating_sub(v)>3_000
            ||t.source_quote_time_ms.is_none_or(|s|s<v))
    {return false}
    let found=t.positions.iter().filter(|p|
        p.identifier==pid||p.ticket==ticket).collect::<Vec<_>>();
    if found.len()!=1{return false}
    let p=found[0];
    let tick=t.tick_size.unwrap_or(f64::NAN);
    let step=t.volume_step.unwrap_or(f64::NAN);
    let min=t.volume_min.unwrap_or(f64::NAN);
    let lots=cmd["lots"].as_f64().unwrap_or(f64::NAN);
    let expected=cmd["expected_position_volume"].as_f64().unwrap_or(f64::NAN);
    let target_price=cmd["tp_target_price"].as_f64().unwrap_or(f64::NAN);
    if p.identifier!=pid||p.ticket!=ticket||p.symbol!=t.symbol
        ||cmd["expected_position_side"]!=p.side
        ||cmd["magic"].as_u64()!=Some(p.magic)
        ||cmd["brain"].as_str()!=execution::brain_for_magic(p.magic)
        ||!matches!(p.side.as_str(),"BUY"|"SELL")
        ||!near(p.price_open,cmd["expected_position_entry"].as_f64()
            .unwrap_or(f64::NAN),tick)
        ||!near(p.tp,cmd["expected_broker_tp"].as_f64().unwrap_or(f64::NAN),tick)
        ||!near(p.volume,expected,step)
        ||!min.is_finite()||min<=0.||!lots.is_finite()
        ||lots<min-step*1e-6||!target_price.is_finite()||target_price<=0.
        ||!((target_price/tick).is_finite()
            &&((target_price/tick)-(target_price/tick).round()).abs()<=1e-6)
        ||!near((lots/step).round()*step,lots,step)
        ||lots>expected+step*1e-6||expected-lots<min-step*1e-6
    {return false}
    match p.side.as_str(){
        "BUY"=>target_price>p.price_open+tick*0.5
            &&t.bid.is_some_and(|bid|bid.is_finite()&&bid>=target_price-tick*1e-7),
        "SELL"=>target_price<p.price_open-tick*0.5
            &&t.ask.is_some_and(|ask|ask.is_finite()&&ask<=target_price+tick*1e-7),
        _=>false
    }
}
pub fn stage_id_exists(db:&Connection,id:&str)->bool{
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM partial_exit_tp1_held WHERE command_id=?1)",
        [id],|r|r.get::<_,i64>(0)
    ).unwrap_or(0)==1
}
fn durable(db:&Connection,id:&str,context:&str,target:&str,
    stage_state:&str,now:u64)->Result<Option<(u64,u64,String,Value)>>{
    let row:Option<(u64,u64,String,String,String,String,String,u64)>=db.query_row(
        "SELECT h.position_identifier,h.physical_ticket,h.plan_id,
                h.immutable_command,h.state,b.payload,b.state,h.created_ms
         FROM partial_exit_tp1_held h JOIN broker_commands b
           ON b.id=h.command_id
         WHERE h.command_id=?1 AND h.context=?2 AND h.target=?3
           AND b.context=?2 AND b.kind='close'",
        params![id,context,target],|r|Ok((
            r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,
            r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?
        ))
    ).optional()?;
    let Some((pid,ticket,plan,raw,held_state,actual,broker_state,at))=row
        else{return Ok(None)};
    let Ok(command)=serde_json::from_str::<Value>(&raw)else{return Ok(None)};
    if held_state!="held_never_dispatched"||broker_state!=stage_state
        ||at==0||at>now||now.saturating_sub(at)>3_000
        ||command["id"]!=id||command["tp_stage_plan_id"]!=plan
        ||serde_json::from_str::<Value>(&actual).ok()!=Some(command.clone())
    {return Ok(None)}
    Ok(Some((pid,ticket,plan,command)))
}
/// Reverify after a staged command has been selected for Gateway work, and
/// again at the final Core authorize callback. No approval by broker ACK.
pub fn authorize(db:&Connection,t:&Telemetry,target:&str,id:&str,
    revision:u64,now:u64,run:bool)->bool{
    if !run||!t.quote_is_fresh(now,3_000){return false}
    let context=key(t);
    let state:Option<String>=db.query_row(
        "SELECT state FROM broker_commands WHERE id=?1 AND context=?2",
        params![id,context],|r|r.get(0)
    ).optional().unwrap_or(None);
    let Some(state)=state else{return false};
    if !matches!(state.as_str(),"queued"|"dispatched"){return false}
    let Ok(Some((pid,ticket,plan,cmd)))=durable(
        db,id,&context,target,&state,now)else{return false};
    let release:Option<(String,u64,u64,String,String,u64)>=db.query_row(
        "SELECT plan_id,position_identifier,physical_ticket,
                immutable_command,state,released_ms
         FROM partial_exit_tp1_releases
         WHERE command_id=?1 AND context=?2 AND target=?3",
        params![id,context,target],|r|Ok((
            r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?
        ))
    ).optional().unwrap_or(None);
    let Some((rplan,rpid,rticket,raw,st,released_ms))=release else{return false};
    rplan==plan&&rpid==pid&&rticket==ticket
        &&st=="released_once"&&released_ms>=cmd["created_ms"].as_u64().unwrap_or(u64::MAX)
        &&released_ms<=now
        &&serde_json::from_str::<Value>(&raw).ok()==Some(cmd.clone())
        &&snapshot_valid(t,target,pid,ticket,&cmd,revision,now)
        &&crate::partial_exit_tp1_arm::eligible(
            db,t,target,pid,ticket,&plan,revision,now).unwrap_or(false)
}
/// Atomic held -> queued with one release receipt; only after a fresh
/// selected-account MT5 snapshot and an independent operator execution arm.
/// Existing queued/dispatched entries are never requeued or rewritten.
pub fn release(db:&Connection,t:&Telemetry,target:&str,revision:u64,
    now:u64,run:bool)->Result<Value>{
    let empty=||json!({"status":"no_verified_release",
        "released":0,"rejected":0,"broker_fill_verified":false});
    if !run||!t.quote_is_fresh(now,3_000){return Ok(empty())}
    let key=key(t);
    let mut stmt=db.prepare(
        "SELECT h.command_id FROM partial_exit_tp1_held h
         JOIN broker_commands b ON b.id=h.command_id
         WHERE h.context=?1 AND h.target=?2 AND b.context=?1
           AND b.state='stage_held'
         ORDER BY h.created_ms DESC LIMIT 25")?;
    let ids=stmt.query_map(params![key,target],|r|r.get::<_,String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut released=0;
    let mut rejected=0;
    for id in ids {
        let Some((pid,ticket,plan,cmd))=durable(
            db,&id,&key,target,"stage_held",now)? else{rejected+=1;continue};
        if !snapshot_valid(t,target,pid,ticket,&cmd,revision,now)
            ||!crate::partial_exit_tp1_arm::eligible(
                db,t,target,pid,ticket,&plan,revision,now).unwrap_or(false)
        {rejected+=1;continue}
        let tx=db.unchecked_transaction()?;
        let arms:i64=tx.query_row(
            "SELECT COUNT(*) FROM partial_exit_tp1_arms
             WHERE context=?1 AND target=?2 AND position_identifier=?3
               AND physical_ticket=?4 AND settings_revision=?5
               AND state='armed_once' AND armed_ms<=?6 AND expires_ms>?6",
            params![key,target,pid,ticket,revision,now],|r|r.get(0))?;
        if arms!=1{rejected+=1;continue}
        let others:i64=tx.query_row(
            "SELECT COUNT(*) FROM broker_commands
             WHERE context=?1 AND id<>?2 AND kind='close'
               AND (NOT json_valid(payload)
                    OR json_extract(payload,'$.position') IS NULL
                    OR CAST(json_extract(payload,'$.position') AS INTEGER)=?3)",
            params![key,id,ticket],|r|r.get(0))?;
        if others!=0{rejected+=1;continue}
        let inserted=tx.execute(
            "INSERT OR IGNORE INTO partial_exit_tp1_releases(
               command_id,context,target,position_identifier,
               physical_ticket,plan_id,released_ms,immutable_command,state)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'released_once')",
            params![id,key,target,pid,ticket,plan,now,cmd.to_string()])?;
        if inserted!=1{rejected+=1;continue}
        let updated=tx.execute(
            "UPDATE broker_commands SET state='queued'
             WHERE id=?1 AND context=?2 AND state='stage_held'
               AND payload=?3",
            params![id,key,cmd.to_string()])?;
        if updated!=1{return Err(anyhow::anyhow!(
            "TP1 durable broker command changed during atomic queue release"))}
        tx.execute(
            "INSERT INTO events(timestamp_ms,kind,detail)
             VALUES(?1,'tp1_core_released_once_to_gateway_queue',?2)",
            params![now,id])?;
        tx.commit()?;
        released+=1;
    }
    Ok(json!({"status":"tp1_explicitly_armed_broker_queue_release",
        "released":released,"rejected":rejected,"broker_fill_verified":false}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_one_shot_release_is_atomic_and_authorization_revocable(){
        use braintrade_domain::Position;
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE broker_commands(
          id TEXT PRIMARY KEY,context TEXT,brain TEXT,kind TEXT,
          created_ms INTEGER,state TEXT,payload TEXT,result TEXT DEFAULT '{}');
          CREATE TABLE events(id INTEGER PRIMARY KEY,
            timestamp_ms INTEGER,kind TEXT,detail TEXT);").unwrap();
        init(&db).unwrap();
        crate::partial_exit_tp1_held::init(&db).unwrap();
        crate::partial_exit_tp1_arm::init(&db).unwrap();
        crate::partial_exit_trade_approval::init(&db).unwrap();
        crate::partial_exit_stage_consent::init(&db).unwrap();
        crate::partial_exit_prices::init(&db).unwrap();
        crate::partial_exit_stage::init(&db).unwrap();
        let t=Telemetry{
            connected:true,demo:true,account_trade_mode:Some(0),
            margin_mode:Some(2),login:17,server:"D".into(),
            symbol:"XAUUSDm".into(),currency:"USD".into(),
            volume_min:Some(0.01),volume_step:Some(0.01),
            volume_max:Some(100.0),tick_size:Some(0.01),
            quote_time_ms:Some(1550),source_quote_time_ms:Some(1500),
            tick_age_ms:Some(0),bid:Some(2005.2),ask:Some(2005.4),
            positions:vec![Position{
                ticket:87,identifier:907,magic:410601,
                symbol:"XAUUSDm".into(),side:"BUY".into(),
                volume:0.10,price_open:2000.0,tp:0.0,
                ..Default::default()
            }],..Default::default()
        };
        let ctx=key(&t);
        let ids=[
            format!("tp-plan-{}","a".repeat(32)),
            format!("tp-plan-{}","b".repeat(32)),
            format!("tp-plan-{}","c".repeat(32))];
        let cap=json!({
            "schema":"partial-exit-price-capture-v1",
            "context":ctx,"target":"demo","position_identifier":907,
            "ticket":87,"magic":410601,"symbol":"XAUUSDm","side":"BUY",
            "entry":2000.0,"original_volume":0.10,"lot_step":0.01,
            "tick_size":0.01,"original_broker_tp":0.0,"saved_ms":1000,
            "saved_bid":2000.5,"saved_ask":2001.0,"quote_reference":2000.5,
            "broker_quote_ms":950,
            "orders_enabled":false,"trading_enabled":false,"breakeven_enabled":false
        });
        db.execute(
            "INSERT INTO partial_exit_price_plans(
             context,target,position_identifier,ticket,magic,
             plan_ids_json,prices_json,captured_json,saved_ms)
             VALUES(?1,'demo',907,87,410601,?2,'[2005.0,2010.0,2015.0]',?3,1000)",
            params![ctx,json!(ids).to_string(),cap.to_string()]
        ).unwrap();
        for (i,id) in ids.iter().enumerate(){
            db.execute(
                "INSERT INTO partial_exit_stage_plans(
                 context,target,position_identifier,stage,plan_id,planned_volume,
                 created_ms,status)
                 VALUES(?1,'demo',907,?2,?3,?4,1050,'prepared_only')",
                params![ctx,i+1,id,[0.05,0.03,0.02][i]]
            ).unwrap();
        }
        db.execute(
            "INSERT INTO partial_exit_stage_consents(
             context,target,position_identifier,position_ticket,plan_ids,
             approved_revision,approved_ms,enabled,purpose)
             VALUES(?1,'demo',907,87,?2,5,1150,1,'preparation_only')",
            params![ctx,json!(ids).to_string()]
        ).unwrap();
        db.execute(
            "INSERT INTO partial_exit_tp1_trade_approvals(
             context,target,position_identifier,physical_ticket,plan_ids,
             settings_revision,approved_ms,expires_ms,enabled,purpose)
             VALUES(?1,'demo',907,87,?2,5,1200,86401200,1,
             'tp1_future_trading_preference')",
            params![ctx,json!(ids).to_string()]
        ).unwrap();
        db.execute(
            "INSERT INTO partial_exit_tp1_arms(
             context,target,position_identifier,physical_ticket,plan_ids,
             settings_revision,armed_ms,expires_ms,state)
             VALUES(?1,'demo',907,87,?2,5,1300,901300,'armed_once')",
            params![ctx,json!(ids).to_string()]
        ).unwrap();
        let brain=execution::brain_for_magic(410601).unwrap();
        let cmd=json!({
            "id":"tp1-once-id","schema":"tp-stage-broker-command-shape-v1",
            "context":ctx,"target":"demo","login":17,"server":"D",
            "currency":"USD","symbol":"XAUUSDm","brain":brain,"magic":410601,
            "kind":"close","position":87,"lots":0.05,"revision":5,
            "tp_stage":1,"tp_stage_plan_id":ids[0],
            "expected_position_identifier":907,"expected_position_side":"BUY",
            "expected_position_volume":0.10,"expected_position_entry":2000.0,
            "expected_broker_tp":0.0,"tp_target_price":2005.0,
            "manual_close":false,"manual_open":false,
            "created_ms":1450,"expires_ms":4450,
            "price_source_quote_ms":1400
        });
        db.execute(
            "INSERT INTO broker_commands(
             id,context,brain,kind,created_ms,state,payload)
             VALUES('tp1-once-id',?1,?2,'close',1450,'stage_held',?3)",
            params![ctx,brain,cmd.to_string()]
        ).unwrap();
        db.execute(
            "INSERT INTO partial_exit_tp1_held(
             context,target,position_identifier,physical_ticket,plan_id,
             command_id,created_ms,immutable_command,state)
             VALUES(?1,'demo',907,87,?2,'tp1-once-id',1450,?3,'held_never_dispatched')",
            params![ctx,ids[0],cmd.to_string()]
        ).unwrap();
        assert!(!authorize(&db,&t,"demo","tp1-once-id",5,1550,true),
            "A held stage cannot be authorized before atomic release");
        assert_eq!(release(&db,&t,"demo",5,1550,false).unwrap()["released"],0);
        assert_eq!(release(&db,&t,"demo",5,1550,true).unwrap()["released"],1);
        assert_eq!(release(&db,&t,"demo",5,1550,true).unwrap()["released"],0,
            "The release is exactly once and does not create a second queued command");
        assert_eq!(db.query_row(
            "SELECT state FROM broker_commands WHERE id='tp1-once-id'",
            [],|r|r.get::<_,String>(0)).unwrap(),"queued");
        assert_eq!(db.query_row(
            "SELECT COUNT(*) FROM partial_exit_tp1_releases",
            [],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert!(stage_id_exists(&db,"tp1-once-id"));
        assert!(authorize(&db,&t,"demo","tp1-once-id",5,1550,true));
        assert!(!authorize(&db,&t,"live","tp1-once-id",5,1550,true));
        assert!(!authorize(&db,&t,"demo","tp1-once-id",6,1550,true));
        assert!(!authorize(&db,&t,"demo","tp1-once-id",5,1550,false));
        db.execute(
            "UPDATE broker_commands SET state='dispatched'
             WHERE id='tp1-once-id'",[]
        ).unwrap();
        assert!(authorize(&db,&t,"demo","tp1-once-id",5,1550,true));
        let mut changed=t.clone();changed.positions[0].volume=0.09;
        assert!(!authorize(&db,&changed,"demo","tp1-once-id",5,1550,true),
            "Physical volume change requires Deal reconciliation, never blind resend");
        changed=t.clone();changed.bid=Some(2004.0);
        assert!(!authorize(&db,&changed,"demo","tp1-once-id",5,1550,true),
            "Current broker Bid must still cross the saved TP1 price");
        db.execute(
            "UPDATE partial_exit_tp1_arms SET state='disarmed'
             WHERE position_identifier=907",[]
        ).unwrap();
        assert!(!authorize(&db,&t,"demo","tp1-once-id",5,1550,true),
            "STOP+operator revoke must prevent any further Core send authority");
    }
    #[test]
    fn orphan_or_held_command_cannot_pass_final_stage_authorization(){
        let db=Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        crate::partial_exit_tp1_held::init(&db).unwrap();
        db.execute_batch("CREATE TABLE broker_commands(
          id TEXT PRIMARY KEY,context TEXT,kind TEXT,payload TEXT,state TEXT);").unwrap();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            margin_mode:Some(2),login:17,server:"D".into(),symbol:"XAUUSD".into(),
            currency:"USD".into(),..Default::default()};
        assert!(!authorize(&db,&t,"demo","fake",5,1550,true));
        assert!(!stage_id_exists(&db,"fake"));
        assert_eq!(release(&db,&t,"demo",5,1550,true).unwrap()["released"],0);
    }
}