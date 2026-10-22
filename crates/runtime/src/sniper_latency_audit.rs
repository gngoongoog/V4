//! Passive latency audit over actual persisted gateway broker-command results.
//! Source is Core SQLite broker_commands, not Paper, MT5 history or quote
//! simulation. Queue timestamps and monotonic send times are different clocks;
//! no end-to-end or broker fill latency claim is made.
use anyhow::Result;
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

const MAX_ROWS:i64=250;
const MIN_DESCRIPTIVE:usize=30;
const MAX_DURATION_MS:f64=600_000.0;
const FIELDS:[&str;9]=[
    "queue_to_gateway_ms","queue_to_send_ms","pre_send_ms",
    "check_ms","authorize_ms","post_journal_authorize_ms",
    "journal_ms","send_ms","total_ms",
];

fn valid_duration(v:&Value, field:&str)->Option<f64>{
    v.get(field)?.as_f64().filter(|n|n.is_finite()&&*n>=0.&&*n<=MAX_DURATION_MS)
}
fn percentile(v:&[f64],fraction:f64)->Option<f64>{
    if v.is_empty(){return None}
    let pos=(v.len()-1) as f64*fraction;
    let lo=pos.floor() as usize;
    let hi=pos.ceil() as usize;
    Some(v[lo]+(v[hi]-v[lo])*(pos-lo as f64))
}
fn stats(mut observations:Vec<f64>)->Value {
    observations.sort_by(f64::total_cmp);
    json!({
        "samples":observations.len(),
        "p50_ms":percentile(&observations,0.50),
        "p95_ms":percentile(&observations,0.95),
        "p99_ms":percentile(&observations,0.99),
        "max_ms":observations.last().copied(),
    })
}
pub fn audit(db:&Connection,context:&str)->Result<Value>{
    if context.is_empty() {
        return Ok(json!({"status":"unbound","source":"persisted_gateway_results_only",
            "sampled_commands":0,"note":"لا يوجد حساب MT5 مربوط؛ لا نستنتج سرعات بدون سياق الحساب."}));
    }
    let mut stmt=db.prepare(
        "SELECT state,result FROM broker_commands
         WHERE context=?1 AND kind='open'
           AND json_extract(payload,'$.trading_mode')='sniper'
         ORDER BY created_ms DESC,id DESC LIMIT ?2"
    )?;
    let rows=stmt.query_map(params![context,MAX_ROWS],
        |r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut counts=BTreeMap::<String,u64>::new();
    let mut durations:BTreeMap<&'static str,Vec<f64>>=
        FIELDS.iter().map(|f|(*f,Vec::new())).collect();
    let mut results_available=0usize;
    let mut complete_timing=0usize;
    let mut incomplete_timing=0usize;
    let mut fast_path_count=0usize;
    let mut non_fast_count=0usize;
    let mut broker_deal_ack=0usize;
    let mut queue_target_observed=0usize;
    let mut queue_target_met=0usize;
    let mut unknown_or_unsettled=0usize;
    let mut codes=BTreeMap::<String,u64>::new();

    for (state,raw) in &rows {
        *counts.entry(state.clone()).or_default()+=1;
        if matches!(state.as_str(),"unknown"|"dispatched"|"placed"|"partial"){
            unknown_or_unsettled+=1;
        }
        let result=raw.as_deref().and_then(|s|serde_json::from_str::<Value>(s).ok());
        let Some(value)=result.filter(|v|v.is_object()) else{
            incomplete_timing+=1;continue;
        };
        results_available+=1;
        match value["fast_path"].as_bool(){
            Some(true)=>fast_path_count+=1,
            Some(false)=>non_fast_count+=1,
            _=>{},
        };
        let has_send=value.get("send_ms").and_then(|_|valid_duration(&value,"send_ms")).is_some();
        let has_queue=valid_duration(&value,"queue_to_send_ms").is_some();
        if has_send&&has_queue {complete_timing+=1;}else{incomplete_timing+=1;}
        for field in FIELDS {
            if let Some(n)=valid_duration(&value,field){durations.get_mut(field).unwrap().push(n);}
        }
        if let Some(ms)=valid_duration(&value,"queue_to_send_ms"){
            queue_target_observed+=1;
            if ms<=20.0 {queue_target_met+=1;}
        }
        if value["deal"].as_u64().is_some_and(|x|x>0){broker_deal_ack+=1;}
        if let Some(code)=value["retcode"].as_i64(){
            *codes.entry(code.to_string()).or_default()+=1;
        }
    }
    let sampled=rows.len();
    let mut metrics=Map::new();
    for (field,observations) in durations{
        metrics.insert(field.to_string(),stats(observations));
    }
    Ok(json!({
        "status":if complete_timing>=MIN_DESCRIPTIVE{"descriptive_gateway_latency_only"}
            else{"insufficient_gateway_timing_evidence"},
        "source":"persisted_gateway_results_only",
        "policy":"sniper_latency_journal_audit_v1",
        "sample_limit":MAX_ROWS,"sampled_commands":sampled,
        "results_available":results_available,
        "complete_queue_plus_send_timing":complete_timing,
        "missing_or_invalid_queue_or_send_timing":incomplete_timing,
        "fast_path_result_count":fast_path_count,
        "non_fast_result_count":non_fast_count,
        "state_counts":counts,
        "unresolved_or_partial_at_snapshot":unknown_or_unsettled,
        "broker_ack_with_deal_ticket":broker_deal_ack,
        "retcode_counts":codes,
        "queue_to_send_le_20ms_observed":queue_target_met,
        "queue_to_send_observed":queue_target_observed,
        "metrics":metrics,
        "minimum_descriptive_sample":MIN_DESCRIPTIVE,
        "account_scoped":true,
        "mode_scoped":"sniper",
        "broker_history_crosschecked":false,
        "true_tick_to_fill_ms":Value::Null,
        "broker_execution_speed_verified":false,
        "learning_or_trading_effect":false,
        "limitations":"Timing values are persisted gateway result fields. queue_to_send_ms uses wall-clock difference, while send_ms measures the order_send call; neither proves exchange execution or exact fill time. Missing and cancelled orders cannot be assigned zero latency. No sampling of all market ticks and no profit or live readiness claim."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db()->Connection {
        let d=Connection::open_in_memory().unwrap();
        d.execute_batch("CREATE TABLE broker_commands(
           id TEXT PRIMARY KEY,context TEXT,brain TEXT,kind TEXT,created_ms INTEGER,
           state TEXT,payload TEXT,result TEXT);").unwrap();d
    }
    fn insert(d:&Connection,id:&str,context:&str,mode:&str,state:&str,result:Option<Value>,created:i64){
        d.execute("INSERT INTO broker_commands VALUES(?1,?2,'system','open',?3,?4,?5,?6)",
            params![id,context,created,state,json!({"trading_mode":mode}).to_string(),
                result.map(|r|r.to_string())]).unwrap();
    }
    #[test]
    fn stored_sniper_only_scopes_account_and_mode_not_paper(){
        let d=db();
        insert(&d,"s","a","sniper","filled",Some(json!({
            "fast_path":true,"deal":100,"retcode":10009,
            "queue_to_send_ms":14,"send_ms":25.,"total_ms":51.})),100);
        insert(&d,"expert","a","expert","filled",Some(json!({"send_ms":500.})),102);
        insert(&d,"other","b","sniper","filled",Some(json!({"send_ms":500.})),103);
        let a=audit(&d,"a").unwrap();
        assert_eq!(a["sampled_commands"],1);
        assert_eq!(a["complete_queue_plus_send_timing"],1);
        assert_eq!(a["queue_to_send_le_20ms_observed"],1);
        assert_eq!(a["broker_ack_with_deal_ticket"],1);
        assert_eq!(a["metrics"]["send_ms"]["p50_ms"],25.);
        assert_eq!(a["true_tick_to_fill_ms"],Value::Null);
        assert_eq!(audit(&d,"b").unwrap()["sampled_commands"],1);
        assert_eq!(audit(&d,"").unwrap()["status"],"unbound");
    }
    #[test]
    fn missing_and_uncertain_results_are_not_zero_ms_or_filled(){
        let d=db();
        insert(&d,"unknown","a","sniper","unknown",None,1);
        insert(&d,"reject","a","sniper","rejected",Some(json!({
            "fast_path":true,"check_ms":0.,"total_ms":1.0})),2);
        insert(&d,"placed","a","sniper","placed",Some(json!({
            "fast_path":true,"queue_to_send_ms":10.,"send_ms":null})),3);
        let a=audit(&d,"a").unwrap();
        assert_eq!(a["sampled_commands"],3);
        assert_eq!(a["complete_queue_plus_send_timing"],0);
        assert_eq!(a["metrics"]["send_ms"]["samples"],0);
        assert_eq!(a["metrics"]["send_ms"]["p50_ms"],Value::Null);
        assert_eq!(a["unresolved_or_partial_at_snapshot"],2);
        assert_eq!(a["broker_ack_with_deal_ticket"],0);
    }
    #[test]
    fn invalid_durations_excluded_not_misread_as_zero(){
        let d=db();
        insert(&d,"corrupt","a","sniper","filled",Some(json!({
            "queue_to_send_ms":-2,"send_ms":-1,"total_ms":1e100})),1);
        let v=audit(&d,"a").unwrap();
        assert_eq!(v["metrics"]["send_ms"]["samples"],0);
        assert_eq!(v["missing_or_invalid_queue_or_send_timing"],1);
        assert_eq!(v["status"],"insufficient_gateway_timing_evidence");
    }
    #[test]
    fn percentile_and_window_bound_are_deterministic(){
        let d=db();
        for i in 0..260{
            insert(&d,&format!("s{i:04}"),"a","sniper","filled",Some(json!({
                "fast_path":true,"queue_to_send_ms":i as f64,"send_ms":i as f64
            })),i);
        }
        let v=audit(&d,"a").unwrap();
        assert_eq!(v["sampled_commands"],250);
        assert_eq!(v["status"],"descriptive_gateway_latency_only");
        assert_eq!(v["metrics"]["send_ms"]["p50_ms"],134.5);
        assert_eq!(v["metrics"]["send_ms"]["p95_ms"],246.55);
        assert_eq!(v["metrics"]["send_ms"]["p99_ms"],256.51);
        assert_eq!(v["queue_to_send_le_20ms_observed"],10);
    }
}
