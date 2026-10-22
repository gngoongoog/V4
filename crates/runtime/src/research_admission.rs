//! Deterministic, synthetic-only market entry admission research model.
//! It NEVER calls MetaTrader, changes persistent configuration, or authorizes a
//! live order. Unknown inputs block a hypothetical entry; even 'eligible'
//! is only a result of this model, not a statement of broker acceptance.
use anyhow::{ensure,Result};
use serde::Deserialize;
use serde_json::{json,Value};
use std::collections::BTreeSet;

const LIMIT_BYTES:usize=1_000_000;
const EPS:f64=1e-8;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    account:String,
    symbol:String,
    quote:Quote,
    contract:Contract,
    limits:Limits,
    #[serde(default)] existing:Vec<Exposure>,
    requests:Vec<Candidate>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Quote {bid:f64,ask:f64,age_ms:u64}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Contract {
    volume_min:f64,volume_max:f64,volume_step:f64,
    tick_size:f64,tick_value_loss:f64,
    estimated_margin_per_lot:Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Limits {
    equity:f64,free_margin:f64,
    max_positions:u32,max_total_lots:f64,max_stop_risk_percent:f64,
    commission_roundtrip_per_lot:f64,slippage_ticks_per_side:f64,
    #[serde(default)] daily_breaker_percent:Option<f64>,
    #[serde(default)] synthetic_day_start_equity:Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Exposure {
    id:String,lots:f64,
    stop_risk_cash:Option<f64>,
}
#[derive(Clone,Copy,Deserialize)]
#[serde(rename_all="snake_case")]
enum Side {Buy,Sell}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {id:String,side:Side,lots:f64,stop_price:f64}

fn valid_id(v:&str)->bool {
    !v.is_empty()&&v.len()<=80&&v.bytes().all(|x|
        x.is_ascii_alphanumeric()||matches!(x,b'_'|b'-'|b'.'|b':'))
}
fn nonnegative(v:f64)->bool{v.is_finite()&&v>=0.&&v<=1_000_000_000.}
fn positive(v:f64)->bool{nonnegative(v)&&v>0.}

fn validate(s:&Scenario)->Result<()>{
    ensure!(valid_id(&s.account)&&valid_id(&s.symbol),
        "Synthetic account/symbol identifier is required");
    ensure!(s.existing.len()<=200&&s.requests.len()<=200,
        "Synthetic portfolio/request list exceeds bounded research limits");
    let q=&s.quote;
    ensure!(positive(q.bid)&&positive(q.ask)&&q.bid<=q.ask,
        "Invalid synthetic bid/ask quote");
    let c=&s.contract;
    ensure!([c.volume_min,c.volume_max,c.volume_step,c.tick_size,c.tick_value_loss]
        .iter().all(|x|positive(*x))
        && c.volume_min<=c.volume_max,
        "Synthetic symbol contract invalid");
    ensure!(c.estimated_margin_per_lot.is_none_or(positive),
        "Estimated margin per lot must be positive or unknown");
    let l=&s.limits;
    ensure!(positive(l.equity)&&nonnegative(l.free_margin)
        && (1..=200).contains(&l.max_positions)
        && positive(l.max_total_lots)
        && positive(l.max_stop_risk_percent)&&l.max_stop_risk_percent<=100.
        && nonnegative(l.commission_roundtrip_per_lot)
        && nonnegative(l.slippage_ticks_per_side),
        "Synthetic portfolio limits or costs invalid");
    ensure!(l.daily_breaker_percent.is_none_or(|v|positive(v)&&v<=100.)
        && l.synthetic_day_start_equity.is_none_or(positive),
        "Invalid synthetic daily breaker evidence");
    let mut ids=BTreeSet::new();
    for p in &s.existing{
        ensure!(valid_id(&p.id)&&positive(p.lots)&&p.lots<=10000.,
            "Invalid existing synthetic exposure");
        ensure!(p.stop_risk_cash.is_none_or(nonnegative),
            "Invalid existing stop risk");
        ensure!(ids.insert(p.id.clone()),"Duplicate synthetic exposure ID");
    }
    for c in &s.requests{
        ensure!(valid_id(&c.id)&&positive(c.lots)&&c.lots<=10000.
            && positive(c.stop_price),"Invalid synthetic entry request");
        ensure!(ids.insert(c.id.clone()),"Duplicate synthetic order/exposure ID");
    }
    Ok(())
}
fn volume_ok(lots:f64,c:&Contract)->bool{
    if lots+EPS<c.volume_min || lots>c.volume_max+EPS {return false}
    let step_count=(lots-c.volume_min)/c.volume_step;
    step_count>=-EPS && (step_count-step_count.round()).abs()<=1e-6
}
fn estimated_stop_risk(r:&Candidate,s:&Scenario)->Option<f64>{
    let entry=match r.side{Side::Buy=>s.quote.ask,Side::Sell=>s.quote.bid};
    let movement=match r.side{Side::Buy=>entry-r.stop_price,Side::Sell=>r.stop_price-entry};
    if movement<=0.||!movement.is_finite(){return None}
    let per_lot=(movement/s.contract.tick_size + 2.*s.limits.slippage_ticks_per_side)
        *s.contract.tick_value_loss + s.limits.commission_roundtrip_per_lot;
    let risk=per_lot*r.lots;
    if nonnegative(risk){Some(risk)}else{None}
}

/// Each accepted hypothetical candidate reserves portfolio/margin capacity
/// for the rest of THIS scenario, without modifying any MT5 or Core state.
pub fn evaluate(body:&[u8])->Result<Value>{
    ensure!(body.len()<=LIMIT_BYTES,"Scenario body exceeds one megabyte");
    let s:Scenario=serde_json::from_slice(body)?;
    validate(&s)?;
    let mut slots=s.existing.len();
    let mut lots:f64=s.existing.iter().map(|p|p.lots).sum();
    let existing_risks:Option<Vec<f64>>=s.existing.iter()
        .map(|p|p.stop_risk_cash).collect();
    let mut stop_risk=existing_risks.as_ref().map(|v|v.iter().sum::<f64>());
    let mut margin_reserved=0.;
    let mut decisions=Vec::new();
    let daily_status=if let Some(pct)=s.limits.daily_breaker_percent{
        match s.limits.synthetic_day_start_equity{
            None=>"unknown",
            Some(baseline)=>if baseline-s.limits.equity>=baseline*pct/100.{"tripped"}
                else{"not_tripped"}
        }
    }else{"disabled"};
    for r in &s.requests{
        let mut reasons=Vec::<&str>::new();
        if s.quote.age_ms>3000{reasons.push("stale_quote");}
        if !volume_ok(r.lots,&s.contract){reasons.push("lot_step_or_min_max");}
        let estimate=estimated_stop_risk(r,&s);
        if estimate.is_none(){reasons.push("invalid_stop_geometry_or_risk");}
        if slots>=s.limits.max_positions as usize {reasons.push("portfolio_position_slots");}
        if lots+r.lots>s.limits.max_total_lots+EPS {reasons.push("portfolio_total_lots");}
        let budget=s.limits.equity*s.limits.max_stop_risk_percent/100.;
        match (stop_risk,estimate) {
            (None,_)=>reasons.push("existing_stop_risk_unknown"),
            (Some(existing),Some(new)) if existing+new>budget+EPS=>
                reasons.push("portfolio_stop_risk_budget"),
            _=>{}
        }
        let needed=s.contract.estimated_margin_per_lot.map(|v|v*r.lots)
            .filter(|v|nonnegative(*v));
        match needed {
            None=>reasons.push("estimated_margin_unknown"),
            Some(v) if v+margin_reserved>s.limits.free_margin+EPS=>
                reasons.push("estimated_margin_exceeds_free"),
            _=>{}
        }
        match daily_status {
            "tripped"=>reasons.push("synthetic_daily_loss_breaker"),
            "unknown"=>reasons.push("synthetic_daily_baseline_unknown"),
            _=>{}
        }
        let eligible=reasons.is_empty();
        if eligible {
            slots+=1;
            lots+=r.lots;
            if let (Some(current),Some(next))=(stop_risk,estimate){
                stop_risk=Some(current+next)
            }
            margin_reserved+=needed.unwrap_or(0.);
        }
        decisions.push(json!({
            "id":r.id,"side":match r.side{Side::Buy=>"buy",Side::Sell=>"sell"},
            "eligible_hypothetical":eligible,"reasons":reasons,
            "requested_lots":r.lots,"entry_price_assumed":
                match r.side{Side::Buy=>s.quote.ask,Side::Sell=>s.quote.bid},
            "requested_stop_price":r.stop_price,
            "estimated_loss_at_stop_including_costs":estimate,
            "estimated_margin_required":needed,
            "broker_authorized":false,
        }));
    }
    let eligible=decisions.iter().filter(|v|v["eligible_hypothetical"]==true).count();
    Ok(json!({
        "policy":"synthetic-admission-v1",
        "status":"synthetic_only",
        "account":s.account,"symbol":s.symbol,
        "decisions":decisions,"eligible":eligible,
        "blocked":s.requests.len()-eligible,
        "existing_exposure_count":s.existing.len(),
        "hypothetical_total_positions":slots,
        "hypothetical_total_lots":lots,
        "hypothetical_total_estimated_stop_risk":stop_risk,
        "hypothetical_new_margin_reserved":margin_reserved,
        "daily_breaker_evidence":daily_status,
        "broker_approvals":0,"mt5_commands":0,"learning_effect":false,
        "limitations":"Synthetic single-symbol portfolio snapshot. Margin is linear externally supplied estimate, stop loss uses current quote and tick-value estimate, and daily equity baseline ignores deposits. No true MT5 order_check, broker stops/freeze levels, netting/hedging, fills, swaps, history reconciliation or complete risk stack. Hypothetical eligibility is never a live approval."
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(requests:&str)->String{
        format!(r#"{{"account":"demo_77","symbol":"XAUUSD",
            "quote":{{"bid":5000.0,"ask":5000.2,"age_ms":50}},
            "contract":{{"volume_min":0.01,"volume_max":10.0,
                "volume_step":0.01,"tick_size":0.1,"tick_value_loss":1.0,
                "estimated_margin_per_lot":250.0}},
            "limits":{{"equity":500.0,"free_margin":300.0,
                "max_positions":2,"max_total_lots":0.1,
                "max_stop_risk_percent":2.0,"commission_roundtrip_per_lot":7.0,
                "slippage_ticks_per_side":1.0}},
            "requests":{requests}}}"#)
    }
    #[test]
    fn sequential_capacity_does_not_create_fake_broker_orders(){
        let v=evaluate(fixture(r#"[
            {"id":"one","side":"buy","lots":0.04,"stop_price":4999.2},
            {"id":"two","side":"buy","lots":0.04,"stop_price":4999.2},
            {"id":"three","side":"buy","lots":0.04,"stop_price":4999.2}]"#).as_bytes()).unwrap();
        assert_eq!(v["eligible"],2);
        assert_eq!(v["blocked"],1);
        assert_eq!(v["hypothetical_total_positions"],2);
        assert_eq!(v["broker_approvals"],0);
        assert_eq!(v["mt5_commands"],0);
    }
    #[test]
    fn roundtrip_commission_and_spread_are_counted_against_entry_stop(){
        let v=evaluate(fixture(r#"[
            {"id":"x","side":"buy","lots":0.01,"stop_price":4999.2}]"#).as_bytes()).unwrap();
        let risk=v["decisions"][0]["estimated_loss_at_stop_including_costs"].as_f64().unwrap();
        // 10 stop ticks + 2 slippage ticks + $7/lot roundtrip charge.
        assert!((risk-0.19).abs()<1e-8);
        assert_eq!(v["decisions"][0]["eligible_hypothetical"],true);
    }
    #[test]
    fn unknown_margin_and_existing_stop_risk_block_instead_of_approve(){
        let mut s:Value=serde_json::from_str(&fixture(r#"[
            {"id":"x","side":"sell","lots":0.01,"stop_price":5000.8}]"#)).unwrap();
        s["contract"]["estimated_margin_per_lot"]=Value::Null;
        s["existing"]=json!([{"id":"p","lots":0.02,"stop_risk_cash":null}]);
        let v=evaluate(s.to_string().as_bytes()).unwrap();
        let arr=v["decisions"][0]["reasons"].as_array().unwrap();
        assert!(arr.iter().any(|x|x=="estimated_margin_unknown"));
        assert!(arr.iter().any(|x|x=="existing_stop_risk_unknown"));
        assert_eq!(v["eligible"],0);
    }
    #[test]
    fn min_lot_and_stale_quotes_are_denials(){
        let mut s:Value=serde_json::from_str(&fixture(r#"[
            {"id":"x","side":"sell","lots":0.01,"stop_price":5000.8}]"#)).unwrap();
        s["contract"]["volume_min"]=json!(0.1);
        s["quote"]["age_ms"]=json!(9000);
        let v=evaluate(s.to_string().as_bytes()).unwrap();
        assert_eq!(v["eligible"],0);
        assert_eq!(v["decisions"][0]["reasons"].as_array().unwrap().len(),2);
    }
    #[test]
    fn daily_breaker_unknown_baseline_or_limit_loss_blocks(){
        let mut s:Value=serde_json::from_str(&fixture(r#"[
            {"id":"x","side":"buy","lots":0.01,"stop_price":4999.2}]"#)).unwrap();
        s["limits"]["daily_breaker_percent"]=json!(2.);
        let unknown=evaluate(s.to_string().as_bytes()).unwrap();
        assert_eq!(unknown["daily_breaker_evidence"],"unknown");
        assert_eq!(unknown["eligible"],0);
        s["limits"]["synthetic_day_start_equity"]=json!(520.);
        let locked=evaluate(s.to_string().as_bytes()).unwrap();
        assert_eq!(locked["daily_breaker_evidence"],"tripped");
        assert_eq!(locked["eligible"],0);
    }
    #[test]
    fn impossible_stop_invalidates_and_does_not_reserve_budget(){
        let v=evaluate(fixture(r#"[
            {"id":"a","side":"buy","lots":0.01,"stop_price":5001.0},
            {"id":"b","side":"sell","lots":0.01,"stop_price":5000.8}]"#).as_bytes()).unwrap();
        assert_eq!(v["blocked"],1);
        assert_eq!(v["eligible"],1);
        assert_eq!(v["hypothetical_total_positions"],1);
    }
    #[test]
    fn rejects_duplicate_intents_and_unknown_properties(){
        let x=fixture(r#"[{"id":"a","side":"buy","lots":0.01,"stop_price":4999.2},
            {"id":"a","side":"buy","lots":0.01,"stop_price":4999.2}]"#);
        assert!(evaluate(x.as_bytes()).is_err());
        let mut s:Value=serde_json::from_str(&fixture("[]")).unwrap();
        s["execute_on_mt5"]=json!(true);
        assert!(evaluate(s.to_string().as_bytes()).is_err());
    }
}
