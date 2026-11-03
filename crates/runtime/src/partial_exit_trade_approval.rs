//! Explicit TP1 TRADE preference, separate from preparatory TP plan consent.
//!
//! This never creates broker_commands or unlocks the existing TP send gate.
//! The next audited phase must consume a fresh, account/plan/volume-matched
//! approval with its own atomic order claim and broker reconciliation.
//! Historical preparation-only approval MUST NOT be upgraded implicitly.
use anyhow::{ensure, Context, Result};
use braintrade_domain::Telemetry;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

const APPROVAL_LIFETIME_MS:u64=86_400_000;

fn context(t:&Telemetry)->String {
    serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
        .unwrap_or_default()
}
fn bound(t:&Telemetry,target:&str)->bool {
    t.connected&&t.login>0&&t.margin_mode==Some(2)&&t.validate()
        &&matches!((target,t.demo,t.account_trade_mode),
            ("demo",true,Some(0))|("live",false,Some(2)))
}
fn valid_ids(ids:&[String;3])->bool {
    ids.iter().all(|id|id.len()==40&&id.starts_with("tp-plan-")
        &&id[8..].bytes().all(|v|v.is_ascii_hexdigit()&&!v.is_ascii_uppercase()))
        &&ids[0]!=ids[1]&&ids[0]!=ids[2]&&ids[1]!=ids[2]
}
pub fn init(db:&Connection)->Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS partial_exit_tp1_trade_approvals(
       context TEXT NOT NULL,
       target TEXT NOT NULL CHECK(target IN ('demo','live')),
       position_identifier INTEGER NOT NULL CHECK(position_identifier>0),
       physical_ticket INTEGER NOT NULL CHECK(physical_ticket>0),
       plan_ids TEXT NOT NULL,
       settings_revision INTEGER NOT NULL CHECK(settings_revision>0),
       approved_ms INTEGER NOT NULL CHECK(approved_ms>0),
       expires_ms INTEGER NOT NULL CHECK(expires_ms>approved_ms),
       enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
       purpose TEXT NOT NULL CHECK(purpose='tp1_future_trading_preference'),
       PRIMARY KEY(context,target,position_identifier)
    );
    CREATE INDEX IF NOT EXISTS tp1_trade_approvals_context
      ON partial_exit_tp1_trade_approvals(context,target,enabled);")?;
    Ok(())
}
fn current_preparation(db:&Connection,t:&Telemetry,target:&str,revision:u64,
   pid:u64,ticket:u64,ids:&[String;3])->Result<bool>{
    let report=crate::partial_exit_stage_consent::report_for_revision(
        db,t,target,revision)?;
    Ok(report["context"]==context(t)&&report["target"]==target
        &&report["status"]=="account_scoped_operator_consent_not_execution"
        &&report["rows"].as_array().is_some_and(|rows|rows.iter().any(|r|
            r["position_identifier"].as_u64()==Some(pid)
            &&r["position_ticket"].as_u64()==Some(ticket)
            &&r["plan_ids"]==json!(ids)
            &&r["approved_revision"].as_u64()==Some(revision)
            &&r["operator_approved"]==true
            &&r["state"]=="operator_approved_not_dispatched"
            &&r["purpose"]=="preparation_only"
            &&r["orders_enabled"]==false)))
}
/// Explicit account-bound intention for eventual TP1 automation.
/// The outer route requires STOP, completed reconciliation, fresh binding,
/// and exact typed Live login when enabled.
pub fn set(db:&Connection,t:&Telemetry,target:&str,pid:u64,ids:&[String;3],
           enabled:bool,revision:u64,now:u64)->Result<Value>{
    ensure!(bound(t,target),"حساب MT5 Hedging المختار غير مطابق");
    ensure!(pid>0&&valid_ids(ids)&&revision>0&&now>0,
        "هوية خطة TP أو نسخة الإعدادات غير صحيحة");
    let key=context(t);
    let old:Option<(u64,String,u64,u64,i64,String)>=db.query_row(
        "SELECT physical_ticket,plan_ids,settings_revision,expires_ms,enabled,purpose
         FROM partial_exit_tp1_trade_approvals
         WHERE context=?1 AND target=?2 AND position_identifier=?3",
         params![key,target,pid],|r|Ok((
             r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?
         ))
    ).optional()?;
    if !enabled {
        // Revocation must succeed even if the position disappeared or a quote
        // expired. Requiring current plan prices here would lock out STOP.
        let (_,original,_,_,_,purpose)=old.context(
            "لا يوجد تفويض TP1 محفوظ للإلغاء")?;
        ensure!(purpose=="tp1_future_trading_preference"
            &&serde_json::from_str::<Value>(&original).ok()==Some(json!(ids)),
            "هوية التفويض المطلوب إلغاؤه لا تطابق السجل");
        db.execute(
            "UPDATE partial_exit_tp1_trade_approvals SET enabled=0
             WHERE context=?1 AND target=?2 AND position_identifier=?3",
            params![key,target,pid])?;
    } else {
        ensure!(t.quote_is_fresh(now,3_000),
            "تفويض TP1 يتطلب اقتباس MT5 حديثاً");
        let found=t.positions.iter().filter(|p|p.identifier==pid).collect::<Vec<_>>();
        ensure!(found.len()==1&&found[0].ticket>0,
            "المركز المطلوب غير موجود أو له هوية مكررة");
        let p=found[0];
        ensure!(p.symbol==t.symbol&&
            matches!(p.side.as_str(),"BUY"|"SELL")&&
            braintrade_domain::execution::brain_for_magic(p.magic).is_some(),
            "المركز لا يتبع عقل تداول مسجلاً لهذا الرمز");
        ensure!(current_preparation(db,t,target,revision,pid,p.ticket,ids)?,
            "يجب اعتماد خطة TP التحضيرية المتطابقة أولاً");
        // Existing approval is immutable to a specific broker ticket and the
        // same plan/revision. Re-approval is a new explicit operator action,
        // never an automatic refresh on a resumed service.
        if let Some((ticket,old_ids,old_revision,_,_,purpose))=&old {
            ensure!(*ticket==p.ticket&&*old_revision==revision
                &&purpose=="tp1_future_trading_preference"
                &&serde_json::from_str::<Value>(old_ids).ok()==Some(json!(ids)),
                "تفويض TP1 القديم مختلف؛ ألغِه وراجع الخطط من جديد");
        }
        let expires=now.checked_add(APPROVAL_LIFETIME_MS)
            .context("Invalid TP1 approval time")?;
        db.execute(
            "INSERT INTO partial_exit_tp1_trade_approvals
             (context,target,position_identifier,physical_ticket,plan_ids,
              settings_revision,approved_ms,expires_ms,enabled,purpose)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1,'tp1_future_trading_preference')
             ON CONFLICT(context,target,position_identifier) DO UPDATE SET
              approved_ms=excluded.approved_ms,expires_ms=excluded.expires_ms,
              enabled=1",
            params![key,target,pid,p.ticket,json!(ids).to_string(),
                revision,now,expires])?;
    }
    Ok(json!({"ok":true,"trade_preference_saved":enabled,
        "context":key,"target":target,"position_identifier":pid,
        "purpose":"tp1_future_trading_preference",
        "tp1_execution_enabled":false,"orders_enabled":false,
        "broker_commands_created":0,
        "state":if enabled{"trade_preference_stored_sender_locked"}
            else{"trade_preference_revoked"}}))
}
/// This is a read-only CURRENT eligibility assessment. Persistent approval
/// does not imply an actual broker order, even if still time-valid.
pub fn report(db:&Connection,t:&Telemetry,target:&str,now:u64,
    revision:u64)->Result<Value>{
    let mut result=json!({"schema":"tp1-trade-approval-v1",
        "status":"bound_account_required","rows":[],"read_only":true,
        "tp1_execution_enabled":false,"broker_commands_created":0});
    if !bound(t,target){return Ok(result)}
    let key=context(t);
    let mut stmt=db.prepare(
        "SELECT position_identifier,physical_ticket,plan_ids,
                settings_revision,approved_ms,expires_ms,enabled,purpose
         FROM partial_exit_tp1_trade_approvals
         WHERE context=?1 AND target=?2 ORDER BY approved_ms DESC LIMIT 50")?;
    let records=stmt.query_map(params![key,target],|r|Ok((
        r.get::<_,u64>(0)?,r.get::<_,u64>(1)?,r.get::<_,String>(2)?,
        r.get::<_,u64>(3)?,r.get::<_,u64>(4)?,r.get::<_,u64>(5)?,
        r.get::<_,i64>(6)?,r.get::<_,String>(7)?
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut rows=Vec::new();
    for (pid,ticket,raw,rev,at,expires,enabled,purpose) in records {
        let ids=serde_json::from_str::<[String;3]>(&raw).ok();
        let intact=ids.as_ref().is_some_and(valid_ids)
            &&ticket>0&&rev>0&&at>0&&expires>at
            &&matches!(enabled,0|1)&&purpose=="tp1_future_trading_preference";
        let matching_position=t.positions.iter().filter(|p|
            p.identifier==pid||p.ticket==ticket).collect::<Vec<_>>();
        let lineage=intact&&revision==rev&&ids.as_ref().is_some_and(|ids|
            matching_position.len()==1
            &&matching_position[0].identifier==pid
            &&matching_position[0].ticket==ticket
            &&current_preparation(db,t,target,revision,pid,ticket,ids).unwrap_or(false));
        let fresh=lineage&&t.quote_is_fresh(now,3_000);
        let unexpired=now>=at&&now<expires;
        let active=intact&&enabled==1&&fresh&&unexpired;
        let state=if !intact{"invalid_trade_preference"}
            else if enabled==0{"trade_preference_revoked"}
            else if revision!=rev{"trade_preference_revision_mismatch"}
            else if !lineage{"trade_preference_lineage_stale"}
            else if !unexpired{"trade_preference_expired"}
            else if !fresh{"trade_preference_quote_stale"}
            else{"trade_preference_current_sender_locked"};
        rows.push(json!({"position_identifier":pid,"physical_ticket":ticket,
            "plan_ids":if intact{json!(ids)}else{Value::Null},
            "settings_revision":rev,"approved_ms":at,"expires_ms":expires,
            "trade_preference_current":active,"state":state,
            "purpose":purpose,"read_only":true,
            "tp1_execution_enabled":false,"orders_enabled":false,
            "broker_command_id":Value::Null}));
    }
    result["status"]=json!("account_scoped_trade_preference_not_execution");
    result["context"]=json!(key);result["target"]=json!(target);
    result["rows"]=json!(rows);
    Ok(result)
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn no_implicit_approval_or_cross_account_trade_permission(){
        let db=Connection::open_in_memory().unwrap();init(&db).unwrap();
        let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
            margin_mode:Some(2),login:50,server:"DemoA".into(),
            symbol:"XAUUSDm".into(),currency:"USD".into(),..Default::default()};
        let ids=[format!("tp-plan-{}","a".repeat(32)),
            format!("tp-plan-{}","b".repeat(32)),
            format!("tp-plan-{}","c".repeat(32))];
        let report=report(&db,&t,"demo",1000,7).unwrap();
        assert_eq!(report["rows"].as_array().unwrap().len(),0);
        assert_eq!(report["tp1_execution_enabled"],false);
        assert!(set(&db,&t,"demo",907,&ids,true,7,1000).is_err());
        assert!(set(&db,&t,"live",907,&ids,true,7,1000).is_err());
        assert_eq!(db.query_row(
            "SELECT COUNT(*) FROM partial_exit_tp1_trade_approvals",
            [],|r|r.get::<_,i64>(0)).unwrap(),0);
    }
    #[test]
    fn stored_permission_restarts_revokes_even_when_position_gone_and_never_queues(){
        let file=std::env::temp_dir().join(format!(
            "bt-tp1-optin-{}.sqlite",uuid::Uuid::new_v4().simple()));
        let key=r#"["DemoA",50,"XAUUSDm","USD"]"#;
        let ids=[format!("tp-plan-{}","a".repeat(32)),
            format!("tp-plan-{}","b".repeat(32)),
            format!("tp-plan-{}","c".repeat(32))];
        {
            let db=Connection::open(&file).unwrap();
            init(&db).unwrap();
            db.execute("INSERT INTO partial_exit_tp1_trade_approvals VALUES(
                ?1,'demo',907,87,?2,7,1000,2000,1,'tp1_future_trading_preference')",
                params![key,json!(ids).to_string()]).unwrap();
        }
        {
            let db=Connection::open(&file).unwrap();init(&db).unwrap();
            let t=Telemetry{connected:true,demo:true,account_trade_mode:Some(0),
                margin_mode:Some(2),login:50,server:"DemoA".into(),
                symbol:"XAUUSDm".into(),currency:"USD".into(),..Default::default()};
            let read=report(&db,&t,"demo",2001,7).unwrap();
            assert_eq!(read["rows"][0]["trade_preference_current"],false);
            assert_eq!(read["rows"][0]["tp1_execution_enabled"],false);
            assert_eq!(report(&db,&t,"live",2001,7).unwrap()["status"],
                "bound_account_required");
            let revoked=set(&db,&t,"demo",907,&ids,false,7,2001).unwrap();
            assert_eq!(revoked["state"],"trade_preference_revoked");
            assert_eq!(report(&db,&t,"demo",2001,7).unwrap()["rows"][0]["state"],
                "trade_preference_revoked");
            let rows:i64=db.query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='table' AND name='broker_commands'",
                [],|r|r.get(0)).unwrap();
            assert_eq!(rows,0);
        }
        let _=std::fs::remove_file(file);
    }
}