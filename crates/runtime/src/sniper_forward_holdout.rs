//! P1-K5F: predeclared, descriptive time-split diagnostic on VERIFIED
//! prospective Demo broker midquote events. Never train, place orders,
//! classify a test as independently audited OOS or claim executable PnL.
use anyhow::{ensure, Result};
use rusqlite::Connection;
use serde_json::{json,Value};
use std::collections::BTreeSet;

const DAY_MS:u64=86_400_000;
const EMBARGO_MS:u64=30*60_000;
const MIN_DAYS:usize=6;
const HOLDOUT_DAYS:usize=2;
const MIN_CALIBRATION:u64=50;
const MIN_HOLDOUT:u64=100;

#[derive(Default)]
struct Score{n:u64,correct:u64,opposite:u64,flat:u64,buy_baseline_correct:u64,
    buy_baseline_opposite:u64,buy_baseline_flat:u64}
impl Score{
 fn sample(&mut self,dir:i64,move_mid:f64,tick:f64) {
     self.n+=1;
     let signed=move_mid*(dir as f64);
     if signed>=tick{self.correct+=1}
     else if signed<=-tick{self.opposite+=1}
     else{self.flat+=1}
     if move_mid>=tick{self.buy_baseline_correct+=1}
     else if move_mid<=-tick{self.buy_baseline_opposite+=1}
     else{self.buy_baseline_flat+=1}
 }
 fn view(&self)->Value{
     json!({"matured_labels":self.n,
       "frozen_correct":self.correct,"frozen_opposite":self.opposite,
       "frozen_flat":self.flat,
       "always_buy_same_opportunities_correct":self.buy_baseline_correct,
       "always_buy_same_opportunities_opposite":self.buy_baseline_opposite,
       "always_buy_same_opportunities_flat":self.buy_baseline_flat,
       "frozen_direction_fraction":if self.n>0{Some(self.correct as f64/self.n as f64)}else{None},
       "always_buy_direction_fraction":if self.n>0{Some(self.buy_baseline_correct as f64/self.n as f64)}else{None}})
 }
}
fn not_ready(reason:&str)->Value{
 json!({"schema":"p1k5f-chronological-prospective-diagnostic-v1",
     "status":"insufficient_or_unverified","reason":reason,
     "independent_oos":false,"broker_pnl":Value::Null,"strategy_win_rate":Value::Null,
     "no_broker_orders":true})
}
/// Caller supplies only valid, reconciled matured event rows from the local
/// SHA-verified forward journal. This split is fixed in source, NOT fitted to
/// the observed labels. Calibration means summary only, NOT model learning.
pub fn diagnose(events:&[Value],cutoff:u64)->Value {
    if events.is_empty(){return not_ready("no_matured_future_events")}
    let mut sessions=BTreeSet::new();
    for e in events {
        let (Some(open),Some(close))=(e["opened_broker_ms"].as_u64(),
            e["closed_broker_ms"].as_u64())else{return not_ready("bad_event_clock")};
        if e["kind"]!="matured"||e["schema"]!="p1k5e-frozen-future-event-v1"
            ||open<=cutoff||open<1_600_000_000_000||close<open.saturating_add(5000)
            ||close/DAY_MS!=open/DAY_MS {
            return not_ready("invalid_or_cross_session_event");
        }
        let Some(movement)=e["midquote_change"].as_f64()else{
            return not_ready("missing_midquote_change")};
        let Some(threshold)=e["threshold_tick"].as_f64()else{
            return not_ready("missing_tick_threshold")};
        let Some(side)=e["side"].as_i64()else{return not_ready("missing_frozen_side")};
        if !movement.is_finite()||!threshold.is_finite()||threshold<=0.
            ||!matches!(side,-1|1) {return not_ready("invalid_direction_evidence")}
        let signed=movement*(side as f64);
        let predicted=if signed>=threshold{"correct"}
            else if signed<=-threshold{"opposite"}else{"flat"};
        if e["outcome"]!=predicted {return not_ready("journal_direction_mismatch")}
        sessions.insert(open/DAY_MS);
    }
    if sessions.len()<MIN_DAYS{return not_ready("need_six_distinct_utc_broker_days")}
    let boundary_day=*sessions.iter().nth(sessions.len()-HOLDOUT_DAYS)
        .expect("six days implies held-out day");
    let boundary=boundary_day.saturating_mul(DAY_MS);
    let mut calibration=Score::default();
    let mut holdout=Score::default();
    let mut embargoed=0u64;
    let mut caldays=BTreeSet::new();let mut testdays=BTreeSet::new();
    for e in events {
       let open=e["opened_broker_ms"].as_u64().unwrap_or(0);
       let close=e["closed_broker_ms"].as_u64().unwrap_or(0);
       let side=e["side"].as_i64().unwrap_or(0);
       let movement=e["midquote_change"].as_f64().unwrap_or(0.);
       let tick=e["threshold_tick"].as_f64().unwrap_or(0.);
       if close>=boundary.saturating_sub(EMBARGO_MS)
            &&open<=boundary.saturating_add(EMBARGO_MS) {
           embargoed+=1;continue;
       }
       if close<boundary.saturating_sub(EMBARGO_MS) {
           calibration.sample(side,movement,tick);caldays.insert(open/DAY_MS);
       } else if open>boundary.saturating_add(EMBARGO_MS){
           holdout.sample(side,movement,tick);testdays.insert(open/DAY_MS);
       } else {embargoed+=1;}
    }
    if caldays.len()<MIN_DAYS-HOLDOUT_DAYS
        ||testdays.len()<HOLDOUT_DAYS
        ||calibration.n<MIN_CALIBRATION||holdout.n<MIN_HOLDOUT{
        return json!({"schema":"p1k5f-chronological-prospective-diagnostic-v1",
          "status":"insufficient_or_unverified",
          "reason":"not_enough_nonoverlapping_sessions_or_labels",
          "calibration_days":caldays.len(),"holdout_days":testdays.len(),
          "calibration_labels":calibration.n,"holdout_labels":holdout.n,
          "embargoed_labels":embargoed,
          "independent_oos":false,"broker_pnl":Value::Null,
          "strategy_win_rate":Value::Null,"no_broker_orders":true});
    }
    json!({"schema":"p1k5f-chronological-prospective-diagnostic-v1",
      "status":"descriptive_holdout_only",
      "training_data_cutoff_ms":cutoff,
      "split_policy":"last_two_of_at_least_six_observed_utc_broker_days",
      "holdout_boundary_ms":boundary,
      "embargo_ms":EMBARGO_MS,"calibration_days":caldays.len(),
      "holdout_days":testdays.len(),"embargoed_labels":embargoed,
      "calibration":calibration.view(),"holdout":holdout.view(),
      "baseline":"always_buy_at_same_frozen_signal_opportunities",
      "baseline_selection_bias":"Shares event timestamps selected by frozen Learner, not a market-wide naive strategy.",
      "min_calibration_labels":MIN_CALIBRATION,"min_holdout_labels":MIN_HOLDOUT,
      "independent_oos":false,"broker_pnl":Value::Null,
      "strategy_win_rate":Value::Null,"no_broker_orders":true,
      "automatic_promotion":false,"complete_market_tick_coverage":false,
      "caveat":"Same frozen model and selected prospective events, no realized fills, commissions, spread, slippage or independent broker-tick completeness. Not an OOS certification or trading win rate."})
}
/// Never make unverifiable/corrupt rows look like valid evaluation data.
/// Fail-closed report and checksum constraints come from P1-K5D/E.
pub fn from_journal(db:&Connection,account:&str,artifact_id:&str,
                    hash:&str)->Result<Value> {
   let prior=crate::sniper_forward_journal::report(db,account,artifact_id,hash)?;
   ensure!(prior["integrity"]=="verified_local",
       "Cannot time-split non-verifiable forward journal");
   let proof=crate::sniper_research_artifact::verify(db,account,artifact_id,Some(hash))?;
   let cutoff=proof["cutoff_ms"].as_u64().unwrap_or(0);
   let matured=crate::sniper_forward_journal::validated_maturities(
       db,account,artifact_id,hash)?;
   Ok(diagnose(&matured,cutoff))
}
#[cfg(test)]
mod tests{
use super::*;
fn event(day:u64,k:u64,side:i64,change:f64)->Value{
 let t=1_750_000_000_000/DAY_MS*DAY_MS+day*DAY_MS+60*60*1000+k*60_000;
 let signed=change*side as f64;
 json!({"schema":"p1k5e-frozen-future-event-v1",
  "kind":"matured","opened_broker_ms":t,"closed_broker_ms":t+5000,
  "side":side,"midquote_change":change,"threshold_tick":0.01,
  "outcome":if signed>=0.01{"correct"}else if signed<=-0.01{"opposite"}else{"flat"}})
}
#[test]fn six_days_and_purged_fixed_split_yields_descriptive_not_profit(){
 let mut x=Vec::new();
 for d in 0..6{for i in 0..65{
     x.push(event(d,i,if i%2==0{1}else{-1},if i%3==0{0.2}else{-0.2}))
 }}
 let r=diagnose(&x,1_740_000_000_000);
 assert_eq!(r["status"],"descriptive_holdout_only");
 assert_eq!(r["calibration_days"],4);
 assert_eq!(r["holdout_days"],2);
 assert_eq!(r["holdout"]["matured_labels"],130);
 assert_eq!(r["holdout"]["always_buy_same_opportunities_correct"],44);
 assert_eq!(r["independent_oos"],false);
 assert_eq!(r["broker_pnl"],Value::Null);
 assert_eq!(r["strategy_win_rate"],Value::Null);
 assert_eq!(r["automatic_promotion"],false);
}
#[test]fn session_boundary_label_mutation_and_missing_days_fail_closed(){
 let mut x=Vec::new();
 for d in 0..3{for i in 0..60{x.push(event(d,i,1,0.2))}}
 assert_eq!(diagnose(&x,1_740_000_000_000)["status"],"insufficient_or_unverified");
 let mut expanded=Vec::new();
 for d in 0..6{for i in 0..60{expanded.push(event(d,i,1,0.2))}}
 expanded[0]["outcome"]=json!("opposite");
 assert_eq!(diagnose(&expanded,1_740_000_000_000)["reason"],"journal_direction_mismatch");
 expanded[0]["outcome"]=json!("correct");
 let base=expanded[0]["opened_broker_ms"].as_u64().unwrap();
 expanded[0]["closed_broker_ms"]=json!(base+300);
 assert_eq!(diagnose(&expanded,1_740_000_000_000)["reason"],"invalid_or_cross_session_event");
}
}