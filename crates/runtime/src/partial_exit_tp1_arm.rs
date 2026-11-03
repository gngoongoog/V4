//! Explicit, expiring one-position operator arm for an executable TP1.
//! This is NOT the original preparation approval nor the future trade
//! preference. Both are prerequisites; neither implicitly sends a broker order.
//! Arm is session-local intent bounded to the original physical position,
//! exact stage IDs, Core revision and explicitly selected Demo or Live account.
use anyhow::{ensure,Context,Result};
use braintrade_domain::{execution,Telemetry};
use rusqlite::{params,Connection,OptionalExtension};
use serde_json::{json,Value};

const ARM_MAX_MS:u64=15*60_000;
fn context(t:&Telemetry)->String{
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
fn account_ok(t:&Telemetry,target:&str)->bool{
    t.connected&&t.login>0&&t.margin_mode==Some(2)&&t.validate()
        &&matches!((target,t.demo,t.account_trade_mode),
            ("demo",true,Some(0))|("live",false,Some(2)))
}
fn ids_valid(ids:&[String;3])->bool{
    ids.iter().all(|s|s.len()==40&&s.starts_with("tp-plan-")
        &&s[8..].bytes().all(|ch|ch.is_ascii_hexdigit()&&!ch.is_ascii_uppercase()))
        &&ids[0]!=ids[1]&&ids[0]!=ids[2]&&ids[1]!=ids[2]
}
pub fn init(db:&Connection)->Result<()>{
    db.execute_batch("CREATE TABLE IF NOT EXISTS partial_exit_tp1_arms(
      context TEXT NOT NULL,
      target TEXT NOT NULL CHECK(target IN ('demo','live')),
      position_identifier INTEGER NOT NULL CHECK(position_identifier>0),
      physical_ticket INTEGER NOT NULL CHECK(physical_ticket>0),
      plan_ids TEXT NOT NULL,
      settings_revision INTEGER NOT NULL CHECK(settings_revision>0),
      armed_ms INTEGER NOT NULL CHECK(armed_ms>0),
      expires_ms INTEGER NOT NULL CHECK(expires_ms>armed_ms),
      state TEXT NOT NULL CHECK(state IN ('armed_once','disarmed','consumed')),
      PRIMARY KEY(context,target,position_identifier)
    );
    CREATE INDEX IF NOT EXISTS tp1_arms_active
      ON partial_exit_tp1_arms(context,target,state,expires_ms);")?;
    Ok(())
}
fn trade_preference(db:&Connection,t:&Telemetry,target:&str,revision:u64,
    pid:u64,ticket:u64,ids:&[String;3],now:u64)->Result<bool>{
    let report=crate::partial_exit_trade_approval::report(
        db,t,target,now,revision)?;
    Ok(report["context"]==context(t)&&report["target"]==target
        &&report["status"]=="account_scoped_trade_preference_not_execution"
        &&report["rows"].as_array().is_some_and(|rows|rows.iter().any(|r|
            r["position_identifier"].as_u64()==Some(pid)
            &&r["physical_ticket"].as_u64()==Some(ticket)
            &&r["plan_ids"]==json!(ids)
            &&r["trade_preference_current"]==true
            &&r["state"]=="trade_preference_current_sender_locked")))
}
/// Set only after STOP + clean reconciliation in the parent Core route.
/// A Live arm also requires the exact typed account login in that route.
pub fn set(db:&Connection,t:&Telemetry,target:&str,pid:u64,ids:&[String;3],
    enabled:bool,revision:u64,now:u64)->Result<Value>{
    ensure!(account_ok(t,target)&&pid>0&&ids_valid(ids)
        &&revision>0&&now>0,"TP1 arm: account, position or plan invalid");
    let key=context(t);
    let old:Option<(u64,String,u64,String)>=db.query_row(
        "SELECT physical_ticket,plan_ids,settings_revision,state
         FROM partial_exit_tp1_arms
         WHERE context=?1 AND target=?2 AND position_identifier=?3",
        params![key,target,pid],|r|Ok((
            r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?
        ))
    ).optional()?;
    if !enabled {
        let (_,prior_ids,_,_)=old.context("No matching TP1 execution arm to revoke")?;
        ensure!(serde_json::from_str::<Value>(&prior_ids).ok()==Some(json!(ids)),
            "TP1 revoke does not match the original plan");
        db.execute(
            "UPDATE partial_exit_tp1_arms SET state='disarmed'
             WHERE context=?1 AND target=?2 AND position_identifier=?3 AND state='armed_once'",
            params![key,target,pid])?;
    } else {
        ensure!(t.quote_is_fresh(now,3_000),"Fresh MT5 quote required to arm TP1");
        let matching=t.positions.iter().filter(|p|p.identifier==pid).collect::<Vec<_>>();
        ensure!(matching.len()==1&&matching[0].ticket>0,"Missing physical TP1 position");
        let position=matching[0];
        ensure!(position.symbol==t.symbol
            &&execution::brain_for_magic(position.magic).is_some(),
            "TP1 arm: position is not owned by the selected strategy");
        ensure!(trade_preference(db,t,target,revision,pid,position.ticket,ids,now)?,
            "Separate explicit, current trade preference is required");
        // Do not re-arm a consumed or stale plan. An idempotent repeated
        // POST is NOT consent to a second broker order.
        if let Some((ticket,old_ids,old_rev,state))=&old{
            ensure!(*ticket==position.ticket&&*old_rev==revision
                &&matches!(state.as_str(),"armed_once"|"disarmed")
                &&serde_json::from_str::<Value>(old_ids).ok()==Some(json!(ids)),
                "TP1 execution arm is consumed or the original position/plan changed");
        }
        let ends=now.checked_add(ARM_MAX_MS).context("Invalid expiry")?;
        let previous:Option<String>=db.query_row(
            "SELECT command_id FROM partial_exit_tp1_held
             WHERE context=?1 AND target=?2 AND position_identifier=?3",
            params![key,target,pid],|r|r.get(0)
        ).optional()?;
        ensure!(previous.is_none(),
            "TP1 already has a held/issued Core command; cannot arm a second order");
        if let Some((_,_,_,state))=&old {
            // The operator must disarm a stale arm before deliberately
            // arming again. A double click never silently extends expiry.
            if state=="disarmed" {
                db.execute(
                    "UPDATE partial_exit_tp1_arms
                     SET armed_ms=?4,expires_ms=?5,state='armed_once'
                     WHERE context=?1 AND target=?2 AND position_identifier=?3
                       AND state='disarmed'",
                    params![key,target,pid,now,ends])?;
            }
        }else{
            db.execute(
                "INSERT INTO partial_exit_tp1_arms(
                 context,target,position_identifier,physical_ticket,plan_ids,
                 settings_revision,armed_ms,expires_ms,state)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'armed_once')",
                 params![key,target,pid,position.ticket,json!(ids).to_string(),
                     revision,now,ends])?;
        }
    }
    Ok(json!({"ok":true,"armed":enabled,
        "context":key,"target":target,"position_identifier":pid,
        "state":if enabled{"armed_once_not_queued"}else{"disarmed"},
        "broker_commands_created":0,"orders_enabled":false,
        "note":"The arm is not a broker order. A separately validated Core stage release is required."}))
}
/// Must be rechecked immediately before the atomic Core held->queued claim,
/// again at Gateway work selection and again at final send authorization.
pub fn eligible(db:&Connection,t:&Telemetry,target:&str,pid:u64,ticket:u64,
    plan:&str,revision:u64,now:u64)->Result<bool>{
    if !account_ok(t,target)||!t.quote_is_fresh(now,3_000)||revision==0
        ||pid==0||ticket==0||now==0
    {return Ok(false)}
    let key=context(t);
    let current:Option<(u64,String,u64,u64,u64,String)>=db.query_row(
        "SELECT physical_ticket,plan_ids,settings_revision,armed_ms,expires_ms,state
         FROM partial_exit_tp1_arms
         WHERE context=?1 AND target=?2 AND position_identifier=?3",
        params![key,target,pid],|r|Ok((
            r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?
        ))
    ).optional()?;
    let Some((old_ticket,raw,rev,at,ends,state))=current else{return Ok(false)};
    let ids=serde_json::from_str::<[String;3]>(&raw).ok();
    Ok(state=="armed_once"&&old_ticket==ticket&&rev==revision
       &&at>0&&at<=now&&now<ends&&ends<=at.saturating_add(ARM_MAX_MS)
       &&ids.as_ref().is_some_and(|ids|ids_valid(ids)
           &&ids[0]==plan&&trade_preference(
               db,t,target,revision,pid,ticket,ids,now).unwrap_or(false)))
}
/// The arm is consumed ONLY after a transactional release to the broker
/// queue; this method only reports its independently stored validity.
pub fn report(db:&Connection,t:&Telemetry,target:&str,revision:u64,now:u64)->Result<Value>{
    let mut value=json!({"schema":"tp1-execution-arm-v1","status":"account_required",
        "rows":[],"read_only":true,"orders_enabled":false});
    if !account_ok(t,target){return Ok(value)}
    let key=context(t);
    let mut st=db.prepare(
        "SELECT position_identifier,physical_ticket,plan_ids,settings_revision,
         armed_ms,expires_ms,state FROM partial_exit_tp1_arms
         WHERE context=?1 AND target=?2 ORDER BY armed_ms DESC LIMIT 50")?;
    let list=st.query_map(params![key,target],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,r.get::<_,String>(2)?,
        r.get::<_,u64>(3)?,r.get::<_,u64>(4)?,r.get::<_,u64>(5)?,
        r.get::<_,String>(6)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    value["status"]=json!("account_scoped_arm_not_broker_fill");
    value["context"]=json!(key);value["target"]=json!(target);
    value["rows"]=json!(list.into_iter().map(|(pid,ticket,raw,rev,at,ends,state)|{
        let ids=serde_json::from_str::<[String;3]>(&raw).ok();
        let good=ids.as_ref().is_some_and(ids_valid)&&ticket>0&&rev>0&&at>0
            &&ends>at&&ends<=at.saturating_add(ARM_MAX_MS);
        let active=good&&state=="armed_once"&&rev==revision&&now>=at&&now<ends
            &&ids.as_ref().is_some_and(|ids|eligible(
                db,t,target,pid,ticket,&ids[0],revision,now).unwrap_or(false));
        json!({"position_identifier":pid,"physical_ticket":ticket,
            "plan_ids":ids,"armed_ms":at,"expires_ms":ends,
            "state":if !good{"corrupt_execution_arm"}
                else if state=="disarmed"{"disarmed"}
                else if state=="consumed"{"consumed_wait_broker_reconciliation"}
                else if !active{"inactive_or_expired_execution_arm"}
                else{"execution_arm_valid_no_send_yet"},
            "armed_active":active,"orders_enabled":false,"read_only":true})
    }).collect::<Vec<_>>());
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cannot_arm_without_a_separate_trade_preference_or_borrow_live(){
        let db=Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        crate::partial_exit_trade_approval::init(&db).unwrap();
        crate::partial_exit_tp1_held::init(&db).unwrap();
        crate::partial_exit_stage_consent::init(&db).unwrap();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            margin_mode:Some(2),login:17,server:"Demo".into(),
            currency:"USD".into(),symbol:"XAUUSD".into(),
            ..Default::default()};
        let ids=[format!("tp-plan-{}","a".repeat(32)),
            format!("tp-plan-{}","b".repeat(32)),
            format!("tp-plan-{}","c".repeat(32))];
        assert!(set(&db,&t,"demo",907,&ids,true,5,1550).is_err());
        assert!(set(&db,&t,"live",907,&ids,true,5,1550).is_err());
        assert_eq!(report(&db,&t,"live",5,1550).unwrap()["status"],
            "account_required");
    }
    #[test]
    fn revoke_works_after_position_gone_and_never_creates_broker_order(){
        let db=Connection::open_in_memory().unwrap();init(&db).unwrap();
        let key=r#"["Demo",17,"XAUUSD","USD"]"#;
        let ids=[format!("tp-plan-{}","a".repeat(32)),
            format!("tp-plan-{}","b".repeat(32)),
            format!("tp-plan-{}","c".repeat(32))];
        db.execute(
            "INSERT INTO partial_exit_tp1_arms VALUES(
             ?1,'demo',907,87,?2,5,1000,901000,'armed_once')",
            params![key,json!(ids).to_string()]).unwrap();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            margin_mode:Some(2),login:17,server:"Demo".into(),
            symbol:"XAUUSD".into(),currency:"USD".into(),..Default::default()};
        let revoked=set(&db,&t,"demo",907,&ids,false,5,2000).unwrap();
        assert_eq!(revoked["state"],"disarmed");
        let state:String=db.query_row(
            "SELECT state FROM partial_exit_tp1_arms WHERE position_identifier=907",
            [],|r|r.get(0)).unwrap();
        assert_eq!(state,"disarmed");
        assert_eq!(report(&db,&t,"demo",5,2000).unwrap()["rows"][0]["armed_active"],false);
    }
}