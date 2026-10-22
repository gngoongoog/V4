//! Read-only, account-scoped risk monitor. Not a substitute for the two
//! independent Core and MT5 pre-send entry checks.
use anyhow::Result;
use braintrade_domain::{Config, Status, Telemetry};
use chrono::{DateTime, FixedOffset, Utc};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

fn day_at(now_ms:u64)->Option<String> {
    let utc=DateTime::<Utc>::from_timestamp_millis(i64::try_from(now_ms).ok()?)?;
    let tz=FixedOffset::east_opt(3*3600)?;
    Some(utc.with_timezone(&tz).format("%Y-%m-%d").to_string())
}
fn key(t:&Telemetry)->String {
    serde_json::to_string(&(&t.server,t.login,&t.currency,t.account_trade_mode)).unwrap()
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
        "state":state,"reason":reason,"day":day,"account":{"login":t.login,"server":t.server,
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
        db.execute_batch("CREATE TABLE daily_loss_locks(account_key TEXT NOT NULL,report_day TEXT NOT NULL,reached_ms INTEGER NOT NULL,loss_amount REAL NOT NULL,PRIMARY KEY(account_key,report_day));").unwrap();
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
        db.execute_batch("CREATE TABLE daily_loss_locks(account_key TEXT NOT NULL,report_day TEXT NOT NULL,reached_ms INTEGER NOT NULL,loss_amount REAL NOT NULL,PRIMARY KEY(account_key,report_day));").unwrap();
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
}
