//! P0-B / P1-F: broker-journal execution diagnostics. Read-only.
//! Counts never imply broker profitability or a successful fill when history is missing.
//! Caller MUST supply a fresh, verified, bound account and exact symbol context.
use anyhow::Result;
use braintrade_domain::Telemetry;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn category(state: &str, result: &Value) -> &'static str {
    if state == "unknown" { return "unknown_execution"; }
    if state == "cancelled" { return "cancelled_before_send_or_reconciliation"; }
    if state != "rejected" { return "not_rejected"; }
    match result.get("retcode").and_then(Value::as_i64) {
        Some(10019) => return "insufficient_margin",
        Some(10014 | 10038) => return "invalid_volume",
        Some(10016) => return "invalid_stops",
        Some(10017 | 10026 | 10027) => return "trading_disabled",
        Some(10004 | 10015 | 10020 | 10021) => return "price_or_quote",
        Some(10018 | 10029) => return "market_closed",
        Some(10024) => return "too_many_requests",
        Some(10030) => return "filling_policy",
        Some(_) => {}
        None => {}
    }
    let reason=result.get("reason").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
    if reason.contains("margin") || reason.contains("free money") { "insufficient_margin" }
    else if reason.contains("risk budget") || reason.contains("risk sizing") ||
        reason.contains("daily breaker") || reason.contains("risk limit") { "risk_policy" }
    else if reason.contains("lot") || reason.contains("volume") { "invalid_volume" }
    else if reason.contains("stop") { "invalid_stops" }
    else if reason.contains("account") || reason.contains("login") ||
        reason.contains("trade mode") { "account_context" }
    else if reason.contains("spread") || reason.contains("quote") ||
        reason.contains("price") { "price_or_quote" }
    else if reason.contains("authorization") || reason.contains("not allowed") { "authorization" }
    else { "broker_or_unclassified" }
}
fn latency_metric(value: &Value, field: &str) -> Option<f64> {
    value.get(field).and_then(Value::as_f64)
        .filter(|v|v.is_finite()&&*v>=0.&&*v<=60_000.)
}
fn percentiles(mut points:Vec<f64>)->Value {
    if points.is_empty() { return json!({"samples":0,"p50_ms":null,"p95_ms":null,"p99_ms":null}); }
    points.sort_by(|a,b|a.total_cmp(b));
    let quantile=|p:f64| -> f64 {let n=points.len();points[((n as f64*p).ceil() as usize).saturating_sub(1).min(n-1)]};
    json!({"samples":points.len(),"p50_ms":quantile(0.50),"p95_ms":quantile(0.95),"p99_ms":quantile(0.99)})
}
pub(crate) fn report(db:&Connection,t:Option<&Telemetry>,now_ms:u64)->Result<Value> {
    let Some(t)=t.filter(|t|t.connected&&t.login>0&&!t.server.is_empty()
        &&!t.symbol.is_empty()&&!t.currency.is_empty()
        &&t.account_trade_mode==Some(if t.demo {0}else{2})) else {
        return Ok(json!({"state":"unavailable","account_verified":false,
            "rows":[],"source":"Core broker_commands, account-scoped",
            "note":"Requires current fresh and execution-bound MT5 account; never show another account's journal"}));
    };
    let key=serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))?;
    let mut stmt=db.prepare(
        "SELECT brain,kind,state,created_ms,result FROM broker_commands          WHERE context=?1 ORDER BY created_ms DESC,id DESC LIMIT 100"
    )?;
    let rows=stmt.query_map([&key],|r|Ok((
        r.get::<_,String>(0)?,r.get::<_,String>(1)?,
        r.get::<_,String>(2)?,r.get::<_,u64>(3)?,r.get::<_,String>(4)?,
    )))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut counts:BTreeMap<String,u64>=BTreeMap::new();
    let mut rejected:BTreeMap<String,u64>=BTreeMap::new();
    let mut listing=Vec::new();
    let mut queue=Vec::new();let mut send=Vec::new();let mut total=Vec::new();
    let mut unknown=0u64;let mut pending=0u64;let mut rejected_count=0u64;
    for (brain,kind,state,created,raw) in &rows {
        *counts.entry(state.clone()).or_default()+=1;
        let result:Value=serde_json::from_str(raw).unwrap_or(Value::Null);
        let classification=category(state,&result);
        if state=="rejected" {
            rejected_count+=1;
            *rejected.entry(classification.into()).or_default()+=1;
        }
        if state=="unknown"{unknown+=1;}
        if matches!(state.as_str(),"queued"|"dispatched"|"unknown"|"placed"|"partial"|"filled"){
            pending+=1;
        }
        if let Some(v)=latency_metric(&result,"queue_to_send_ms"){queue.push(v)}
        if let Some(v)=latency_metric(&result,"send_ms"){send.push(v)}
        if let Some(v)=latency_metric(&result,"total_ms"){total.push(v)}
        if listing.len()<30 {
            // Broker comments may contain secrets or personal data. Never expose
            // the raw reason, payload, order IDs or MT5 account tokens in UI/exports.
            listing.push(json!({"brain":brain,"kind":kind,"state":state,
                "created_ms":created,"category":classification,
                "retcode":result.get("retcode").and_then(Value::as_i64)}));
        }
    }
    Ok(json!({
        "schema":"p1f-execution-quality-v1","state":"observed","read_only":true,
        "account_verified":true,"observed_ms":now_ms,
        "context":key,
        "account":{"login":t.login,"server":t.server,"symbol":t.symbol,
            "currency":t.currency,"mode":t.account_trade_mode},
        "window":"latest_100_intents_current_account_symbol",
        "sample_size":rows.len(),"state_counts":counts,
        "rejected_count":rejected_count,"rejection_categories":rejected,
        "unknown_count":unknown,"unresolved_count":pending,
        "queue_to_send":percentiles(queue),
        "send":percentiles(send),
        "total":percentiles(total),
        "rows":listing,
        "broker_fill_verified":false,
        "source":"Core durable broker_commands journal and Gateway reported timings",
        "limitations":"States are command journal states, not verified closed trades; missing timings have no estimate. Unknown/partial/dispatched do not count as broker rejects. Broker actual fills require deal-history reconciliation."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    fn fixture()->(Connection,Telemetry){
        let db=Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE broker_commands(id TEXT PRIMARY KEY,
            context TEXT NOT NULL,brain TEXT NOT NULL,kind TEXT NOT NULL,
            created_ms INTEGER NOT NULL,state TEXT NOT NULL,payload TEXT NOT NULL,
            result TEXT NOT NULL DEFAULT '{}');").unwrap();
        let t=Telemetry{connected:true,login:75,server:"MT5-REAL".into(),
            symbol:"XAUUSD".into(),currency:"USD".into(),account_trade_mode:Some(2),
            demo:false,..Default::default()};
        (db,t)
    }
    fn insert(db:&Connection,t:&Telemetry,id:&str,state:&str,result:&str){
        let context=serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency)).unwrap();
        db.execute("INSERT INTO broker_commands(id,context,brain,kind,created_ms,state,payload,result) VALUES(?1,?2,'system','open',100,?3,'{}',?4)",
            params![id,context,state,result]).unwrap();
    }
    #[test]
    fn quality_is_read_only_account_bound_and_never_converts_unknown_into_rejected(){
        let (db,mut t)=fixture();
        insert(&db,&t,"one","rejected",r#"{"retcode":10019,"reason":"private broker comment"}"#);
        insert(&db,&t,"two","unknown",r#"{"reason":"send timeout","total_ms":40}"#);
        insert(&db,&t,"three","cancelled",r#"{"reason":"Core authorization revoked"}"#);
        insert(&db,&t,"four","filled",r#"{"send_ms":5,"queue_to_send_ms":12,"total_ms":19}"#);
        let mut other=t.clone();other.login=76;
        insert(&db,&other,"other","rejected",r#"{"retcode":10014}"#);
        let v=report(&db,Some(&t),101).unwrap();
        assert_eq!(v["sample_size"],4);
        assert_eq!(v["rejected_count"],1);
        assert_eq!(v["rejection_categories"]["insufficient_margin"],1);
        assert_eq!(v["unknown_count"],1);
        assert_eq!(v["unresolved_count"],2);
        assert_eq!(v["send"]["samples"],1);
        assert_eq!(v["total"]["samples"],2);
        assert_eq!(v["broker_fill_verified"],false);
        let encoded=v.to_string();
        assert!(!encoded.contains("private broker comment"));
        assert!(!encoded.contains("Core authorization revoked"));
        assert!(!encoded.contains("\"other\""));
        let count:i64=db.query_row("SELECT COUNT(*) FROM broker_commands",[],|r|r.get(0)).unwrap();
        assert_eq!(count,5,"GET must not mutate intent journal");
        t.connected=false;
        assert_eq!(report(&db,Some(&t),102).unwrap()["state"],"unavailable");
        t.connected=true;t.login=76;
        assert_eq!(report(&db,Some(&t),103).unwrap()["sample_size"],1);
        assert_eq!(report(&db,None,104).unwrap()["account_verified"],false);
    }
    #[test]
    fn brokerage_rejection_diagnostics_never_depend_on_private_comments(){
        let (db,t)=fixture();
        insert(&db,&t,"a","rejected",r#"{"retcode":10014,"reason":"secret code"}"#);
        insert(&db,&t,"b","rejected",r#"{"retcode":10016}"#);
        insert(&db,&t,"c","rejected",r#"{"reason":"Risk sizing minimum lot over risk budget"}"#);
        insert(&db,&t,"d","unknown",r#"{"retcode":10019}"#);
        insert(&db,&t,"e","rejected","{corrupt");
        let v=report(&db,Some(&t),123).unwrap();
        assert_eq!(v["rejection_categories"]["invalid_volume"],1);
        assert_eq!(v["rejection_categories"]["invalid_stops"],1);
        assert_eq!(v["rejection_categories"]["risk_policy"],1);
        assert_eq!(v["rejection_categories"]["broker_or_unclassified"],1);
        assert_eq!(v["unknown_count"],1);
        assert_eq!(v["rejected_count"],4);
        assert!(v.to_string().find("secret code").is_none());
    }
    #[test]
    fn percentile_windows_use_real_samples_without_fabricated_zeros(){
        assert_eq!(percentiles(vec![])["p50_ms"],Value::Null);
        assert_eq!(percentiles(vec![1.,2.,3.,100.])["p50_ms"],2.);
        assert_eq!(percentiles(vec![1.,2.,3.,100.])["p95_ms"],100.);
        assert_eq!(percentiles(vec![1.,2.,3.,100.])["samples"],4);
        assert_eq!(category("unknown",&json!({"retcode":10019})),"unknown_execution");
    }
}
