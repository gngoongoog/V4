//! Read-only, account-scoped risk monitor. Not a substitute for the two
//! independent Core and MT5 pre-send entry checks.
use anyhow::Result;
use braintrade_domain::{Config, Status, Telemetry};
use chrono::{DateTime, FixedOffset, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

fn day_at(now_ms:u64)->Option<String> {
    let utc=DateTime::<Utc>::from_timestamp_millis(i64::try_from(now_ms).ok()?)?;
    let tz=FixedOffset::east_opt(3*3600)?;
    Some(utc.with_timezone(&tz).format("%Y-%m-%d").to_string())
}
fn key(t:&Telemetry)->String {
    serde_json::to_string(&(&t.server,t.login,&t.currency,t.account_trade_mode)).unwrap()
}
// Same JSON account identity as execution_legacy::micro_account_key.
// Recovery is permanently scoped by server, login, currency, NOT symbol.
fn micro_key(t:&Telemetry)->String {
    serde_json::to_string(&(&t.server,t.login,&t.currency)).unwrap()
}
fn micro_recovery_state(db:&Connection,t:&Telemetry,c:&Config,now_ms:u64)->Result<Value> {
    let real=t.account_trade_mode==Some(2) && !t.demo && t.login>0
        && !t.server.is_empty() && !t.currency.is_empty();
    let recovery=c.risk.micro_real_recovery_equity;
    let enabled=c.risk.micro_real_temporary;
    let cap=c.risk.micro_real_cap_percent;
    let row:Option<(u64,f64)>=if real {
        db.query_row(
            "SELECT reached_ms,reached_equity FROM micro_real_recovered WHERE account_key=?1",
            [micro_key(t)],|r|Ok((r.get(0)?,r.get(1)?))
        ).optional()?
    } else {None};
    let previously_recovered=row.is_some();
    let current_milestone_reached=real&&t.equity.is_finite()&&t.equity>=recovery;
    // Read-only candidate: this cannot authorize an order or set a recovery latch.
    let candidate=real&&enabled&&!previously_recovered
        &&!current_milestone_reached&&t.equity.is_finite()&&t.equity>0.;
    let state=if !real {"unverified_or_demo"}
        else if previously_recovered {"recovered_latched"}
        else if current_milestone_reached {"milestone_reached"}
        else if !enabled {"disabled"}
        else if candidate {"candidate_only"} else {"unavailable"};
    Ok(json!({
        "schema":"p1e-micro-recovery-v1",
        "read_only":true,
        "observed_ms":now_ms,
        "account":{"server":t.server,"login":t.login,"currency":t.currency,"mode":t.account_trade_mode},
        "state":state,"candidate_only":candidate,
        "configured":enabled,"cap_percent":cap,"recovery_equity":recovery,
        "recovered_persisted":previously_recovered,
        "reached_ms":row.as_ref().map(|r|r.0),
        "reached_equity":row.as_ref().map(|r|r.1),
        "current_milestone_reached":current_milestone_reached,
        "broker_execution_authorized":false,
        "note":"Read-only monitor: candidate does not authorize a trade; Core, Gateway, broker binding, volume, SL and account-wide breaker are checked separately"
    }))
}
pub fn snapshot(db:&Connection,config:&Config,status:&Status,now_ms:u64)->Result<Value> {
    let demo_on=config.risk.daily_breaker_demo_enabled;
    let real_on=config.risk.daily_breaker_real_enabled;
    let pct=config.risk.daily_loss_percent;
    let Some(day)=day_at(now_ms) else {
        return Ok(json!({"state":"unavailable","reason":"ساعة النظام غير صالحة","demo_enabled":demo_on,"real_enabled":real_on,"limit_percent":pct}));
    };
    let Some(t)=status.telemetry.as_ref().filter(|t|status.gateway_fresh && t.connected) else {
        return Ok(json!({"state":"unavailable","reason":"لا توجد قراءة MT5 حديثة وموثوقة","day":day,
            "demo_enabled":demo_on,"real_enabled":real_on,"limit_percent":pct}));
    };
    let c=config.for_account(t);
    let enabled=match t.account_trade_mode {
        Some(0)=>c.risk.daily_breaker_demo_enabled,
        Some(2)=>c.risk.daily_breaker_real_enabled,
        _=>true
    };
    let locked:bool=db.query_row(
        "SELECT EXISTS(SELECT 1 FROM daily_loss_locks WHERE account_key=?1 AND report_day=?2)",
        params![key(t),day],|row|row.get(0))?;
    let valid=t.report_day==day && t.balance.is_finite() && t.balance>0.
        && t.equity.is_finite() && t.equity>0.
        && t.today_net.is_some_and(|v|v.is_finite())
        && t.today_net.is_some_and(|v|t.balance-v>0. && (t.balance-v).is_finite());
    let baseline=if valid {t.today_net.map(|v|t.balance-v)} else {None};
    let loss=baseline.map(|v|v-t.equity);
    let pct_loss=loss.zip(baseline).map(|(v,b)|v/b*100.);
    let breach=pct_loss.is_some_and(|v|v>=c.risk.daily_loss_percent);
    let (state,reason)=if !enabled {
        ("disabled","قاطع الحساب الحالي مطفأ؛ سجل القفل السابق محفوظ ويمكن استعادته")
    } else if locked {
        ("locked","بلغت خسارة اليوم الحد؛ دخول MT5 الجديد ممنوع حتى اليوم التالي")
    } else if !valid {
        ("unavailable","سجل صفقات اليوم أو الرصيد غير موثوق؛ القاطع يرفض الدخول عند التفعيل")
    } else if breach {
        ("breached","الخسارة تجاوزت الحد؛ سيثبت القفل مع فحص التنفيذ التالي")
    } else {
        ("ready","القاطع مفعّل ولم يتجاوز الحساب سقف الخسارة المرصود")
    };
    Ok(json!({
        "state":state,"reason":reason,"day":day,"observed_ms":now_ms,
        "micro_real":micro_recovery_state(db,t,&c,now_ms)?,
        "account":{"login":t.login,"server":t.server,
            "currency":t.currency,"mode":t.account_trade_mode},
        "demo_enabled":c.risk.daily_breaker_demo_enabled,
        "real_enabled":c.risk.daily_breaker_real_enabled,"enabled":enabled,
        "limit_percent":c.risk.daily_loss_percent,"locked":locked,
        "data_valid":valid,"balance":t.balance,"equity":t.equity,
        "today_net":t.today_net,"estimated_day_start_balance":baseline,
        "estimated_day_loss":loss,"estimated_day_loss_percent":pct_loss,
        "allowed_day_loss":baseline.map(|b|b*c.risk.daily_loss_percent/100.),
        "all_symbols":true,"source":"MT5 account equity + today's broker trade-deal net; estimated baseline",
        "learning":"SHADOW/PAPER simulations and brain observations continue regardless of the broker entry circuit breaker",
        "limitations":"Overnight floating positions, deposits and credit adjustments may affect day-start approximation; gaps can exceed limit"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn risk_monitor_is_read_only_and_preserves_locked_state_when_disabled() {
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE daily_loss_locks(account_key TEXT NOT NULL,report_day TEXT NOT NULL,reached_ms INTEGER NOT NULL,loss_amount REAL NOT NULL,PRIMARY KEY(account_key,report_day));
         CREATE TABLE micro_real_recovered(account_key TEXT PRIMARY KEY,reached_ms INTEGER NOT NULL,reached_equity REAL NOT NULL);").unwrap();
        let now=1_729_500_000_000u64;
        let day=day_at(now).unwrap();
        let mut t=Telemetry{connected:true,account_trade_mode:Some(0),demo:true,login:17,
            server:"Demo".into(),currency:"USD".into(),balance:100.,equity:97.,
            today_net:Some(-3.),report_day:day.clone(),..Default::default()};
        let mut status=Status{gateway_fresh:true,telemetry:Some(t.clone()),..Default::default()};
        let mut c=Config::default();
        let disabled=snapshot(&db,&c,&status,now).unwrap();
        assert_eq!(disabled["state"],"disabled");
        assert_eq!(disabled["estimated_day_loss"],6.);
        c.risk.daily_breaker_demo_enabled=true;
        assert_eq!(snapshot(&db,&c,&status,now).unwrap()["state"],"breached");
        db.execute("INSERT INTO daily_loss_locks VALUES(?1,?2,?3,?4)",
            params![key(&t),day,now,3.]).unwrap();
        assert_eq!(snapshot(&db,&c,&status,now).unwrap()["state"],"locked");
        c.risk.daily_breaker_demo_enabled=false;
        let disabled=snapshot(&db,&c,&status,now).unwrap();
        assert_eq!(disabled["state"],"disabled");
        assert_eq!(disabled["locked"],true);
        t.symbol="EURUSD".into();status.telemetry=Some(t.clone());
        c.risk.daily_breaker_demo_enabled=true;
        assert_eq!(snapshot(&db,&c,&status,now).unwrap()["state"],"locked");
        status.gateway_fresh=false;
        assert_eq!(snapshot(&db,&c,&status,now).unwrap()["state"],"unavailable");
        assert_eq!(snapshot(&db,&c,&status,now).unwrap()["locked"],Value::Null);
    }
    #[test]
    fn risk_monitor_distinguishes_real_demo_and_current_baghdad_day() {
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE daily_loss_locks(account_key TEXT NOT NULL,report_day TEXT NOT NULL,reached_ms INTEGER NOT NULL,loss_amount REAL NOT NULL,PRIMARY KEY(account_key,report_day));
         CREATE TABLE micro_real_recovered(account_key TEXT PRIMARY KEY,reached_ms INTEGER NOT NULL,reached_equity REAL NOT NULL);").unwrap();
        let now=1_729_500_000_000u64;
        let mut t=Telemetry{connected:true,account_trade_mode:Some(2),demo:false,login:19,
            server:"Broker".into(),currency:"USD".into(),balance:50.,equity:49.5,
            today_net:Some(-0.5),report_day:day_at(now).unwrap(),..Default::default()};
        let c=Config::default();
        let mut status=Status{gateway_fresh:true,telemetry:Some(t.clone()),..Default::default()};
        assert_eq!(snapshot(&db,&c,&status,now).unwrap()["state"],"ready");
        t.report_day="2020-01-01".into();status.telemetry=Some(t);
        assert_eq!(snapshot(&db,&c,&status,now).unwrap()["state"],"unavailable");
    }
    #[test]
    fn p1e_micro_recovery_reports_irreversible_account_latch_across_symbols() {
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE daily_loss_locks(account_key TEXT NOT NULL,report_day TEXT NOT NULL,reached_ms INTEGER NOT NULL,loss_amount REAL NOT NULL,PRIMARY KEY(account_key,report_day));
            CREATE TABLE micro_real_recovered(account_key TEXT PRIMARY KEY,reached_ms INTEGER NOT NULL,reached_equity REAL NOT NULL);").unwrap();
        let now=1_729_500_000_000u64;
        let mut t=Telemetry{connected:true,account_trade_mode:Some(2),demo:false,login:51,
            server:"RealServer".into(),currency:"USD".into(),symbol:"XAUUSD".into(),
            equity:50.,balance:50.,today_net:Some(0.),
            report_day:day_at(now).unwrap(),..Default::default()};
        let mut config=Config::default();
        config.risk.micro_real_temporary=true;
        config.risk.micro_real_recovery_equity=500.;
        config.risk.micro_real_cap_percent=10.;
        let mut status=Status{gateway_fresh:true,telemetry:Some(t.clone()),..Default::default()};
        let report=snapshot(&db,&config,&status,now).unwrap();
        assert_eq!(report["micro_real"]["state"],"candidate_only");
        assert_eq!(report["micro_real"]["candidate_only"],true);
        assert_eq!(report["micro_real"]["broker_execution_authorized"],false);
        assert_eq!(report["micro_real"]["read_only"],true);
        assert_eq!(report["micro_real"]["cap_percent"],10.);
        assert_eq!(report["micro_real"]["account"]["login"],51);
        // GET never writes to the durable latch; only execution code does.
        t.equity=500.;status.telemetry=Some(t.clone());
        assert_eq!(snapshot(&db,&config,&status,now).unwrap()["micro_real"]["state"],"milestone_reached");
        let count:i64=db.query_row("SELECT COUNT(*) FROM micro_real_recovered",[],|r|r.get(0)).unwrap();
        assert_eq!(count,0);
        db.execute("INSERT INTO micro_real_recovered VALUES(?1,?2,?3)",
            params![micro_key(&t),now,500.]).unwrap();
        t.equity=50.;t.symbol="EURUSD".into();status.telemetry=Some(t.clone());
        let latched=snapshot(&db,&config,&status,now).unwrap();
        assert_eq!(latched["micro_real"]["state"],"recovered_latched");
        assert_eq!(latched["micro_real"]["candidate_only"],false);
        assert_eq!(latched["micro_real"]["recovered_persisted"],true);
        assert_eq!(latched["micro_real"]["reached_equity"],500.);
        t.login=52;status.telemetry=Some(t.clone());
        assert_eq!(snapshot(&db,&config,&status,now).unwrap()["micro_real"]["candidate_only"],true);
        t.account_trade_mode=Some(0);t.demo=true;status.telemetry=Some(t);
        assert_eq!(snapshot(&db,&config,&status,now).unwrap()["micro_real"]["state"],"unverified_or_demo");
        assert_eq!(snapshot(&db,&config,&status,now).unwrap()["micro_real"]["candidate_only"],false);
        status.gateway_fresh=false;
        assert!(snapshot(&db,&config,&status,now).unwrap().get("micro_real").is_none(),
            "stale broker account has no current micro recovery attestation");
    }

}
