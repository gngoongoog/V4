//! P1-K5G — time-predeclared descriptive cost markouts, NOT executions.
//! Price path: BUY ask->bid, SELL bid->ask, commission charged once
//! round-trip per 1.0 reference lot, with configured *hypothetical* slippage
//! on both sides. No fills, stops, rejects, margin or portfolio PnL.
use anyhow::{ensure,Result};
use rusqlite::Connection;
use serde_json::{json,Value};
use std::collections::BTreeSet;

fn unavailable(reason:&str)->Value{
    json!({"schema":"p1k5g-preregistered-cost-evidence-v1",
        "status":"insufficient_or_unverified","reason":reason,
        "broker_pnl":Value::Null,"independent_oos":false,
        "strategy_win_rate":Value::Null,"no_broker_orders":true})
}
#[derive(Default)]
struct Summary{
    n:u64,total_net:f64,total_baseline_net:f64,
    above_break_even:u64,baseline_above_break_even:u64,
}
impl Summary{
 fn record(&mut self,net:f64,baseline:f64){
     self.n+=1;self.total_net+=net;self.total_baseline_net+=baseline;
     if net>0.{self.above_break_even+=1}
     if baseline>0.{self.baseline_above_break_even+=1}
 }
 fn json(&self)->Value{
     json!({"quote_only_samples":self.n,
         "mean_modeled_quote_after_cost_per_1_lot":
            if self.n>0{Some(self.total_net/self.n as f64)}else{None},
         "mean_baseline_buy_after_cost_per_1_lot":
            if self.n>0{Some(self.total_baseline_net/self.n as f64)}else{None},
         "positive_quote_markouts":self.above_break_even,
         "baseline_positive_quote_markouts":self.baseline_above_break_even,
         "real_broker_pnl":Value::Null,
         "filled_orders":0,"trading_win_rate":Value::Null})
 }
}
fn close(a:f64,b:f64)->bool{(a-b).abs()<=1e-7*f64::max(1.,a.abs().max(b.abs()))}
fn priced(e:&Value,commission:f64,slippage_ticks:f64)->Option<(f64,f64)>{
    if e["kind"]!="matured"||e["modeled_only"]!=true||e["broker_fill"]!=false {
        return None;
    }
    let positive=|v:&Value|v.as_f64().filter(|n|n.is_finite()&&*n>0.);
    let valid=|v:&Value|v.as_f64().filter(|n|n.is_finite());
    let (bid,ask,exit_bid,exit_ask)=(
        positive(&e["entry_bid"])?,positive(&e["entry_ask"])?,
        positive(&e["exit_bid"])?,positive(&e["exit_ask"])?);
    if ask<bid || exit_ask<exit_bid{return None}
    let tick=positive(&e["entry_tick_size"])?;
    let loss=positive(&e["tick_value_loss_per_lot"])?;
    let profit=positive(&e["tick_value_profit_per_lot"])?;
    let side=e["side"].as_i64()?;
    if !matches!(side,-1|1){return None}
    let entry=if side==1{ask}else{bid};
    let exit=if side==1{exit_bid}else{exit_ask};
    let price=(exit-entry)*side as f64;
    let value=if price>=0.{profit}else{loss};
    let gross=price/tick*value;
    let modeled_slip=2.*slippage_ticks*loss.max(profit);
    let net=gross-commission-modeled_slip;
    let always_buy=(exit_bid-ask)/tick
        *if exit_bid>=ask{profit}else{loss}
        -commission-modeled_slip;
    if ![gross,modeled_slip,net,always_buy].iter().all(|v|v.is_finite()){return None}
    if !close(valid(&e["gross_quote_markout_price"])?,price)
       ||!close(valid(&e["quote_gross_account_currency_per_lot"])?,gross)
       ||!close(valid(&e["modeled_slippage_account_currency_per_lot"])?,modeled_slip)
       ||!close(valid(&e["commission_roundtrip_account_currency_per_lot"])?,commission)
       ||!close(valid(&e["modeled_net_quote_account_currency_per_lot"])?,net){
        return None
    }
    Some((net,always_buy))
}
/// Fixed 10-calendar-day comparison window then fixed 10-day evaluation.
/// The split, broker costs and sample gates were signed into the original
/// local frozen checkpoint BEFORE any future label. Never move boundary
/// toward profitable days or retune the parameters to the heldout result.
pub fn diagnose(events:&[Value],protocol:&Value,cutoff:u64)->Value{
    if !crate::sniper_preregister::valid(protocol,cutoff) {
        return unavailable("missing_or_changed_freeze_time_protocol")
    }
    let start=protocol["start_ms"].as_u64().unwrap_or(u64::MAX);
    let boundary=protocol["boundary_ms"].as_u64().unwrap_or(u64::MAX);
    let end=protocol["end_exclusive_ms"].as_u64().unwrap_or(0);
    let embargo=crate::sniper_preregister::EMBARGO_MS;
    let commission=protocol["commission_round_trip_per_lot"].as_f64().unwrap_or(f64::NAN);
    let slippage=protocol["slippage_ticks_per_side"].as_f64().unwrap_or(f64::NAN);
    let mut cal=Summary::default();let mut hold=Summary::default();
    let mut caldays=BTreeSet::new();let mut holddays=BTreeSet::new();
    let mut outside=0u64;let mut embargoed=0u64;
    const DAY:u64=86_400_000;
    for e in events {
        let (Some(open),Some(close_ms))=(
            e["opened_broker_ms"].as_u64(),e["closed_broker_ms"].as_u64())
            else{return unavailable("missing_quote_timestamps")};
        if close_ms<open.saturating_add(5_000)
            ||close_ms/DAY!=open/DAY {
            return unavailable("invalid_horizon_or_cross_day")
        }
        if open<start ||close_ms>=end {outside+=1;continue}
        if open<boundary.saturating_add(embargo)
            &&close_ms>=boundary.saturating_sub(embargo) {
            embargoed+=1;continue;
        }
        let Some((net,baseline))=priced(e,commission,slippage)
            else{return unavailable("missing_broker_tick_value_or_inconsistent_cost_evidence")};
        if close_ms<boundary.saturating_sub(embargo){
            cal.record(net,baseline);caldays.insert(open/DAY);
        }else if open>boundary.saturating_add(embargo){
            hold.record(net,baseline);holddays.insert(open/DAY);
        }else{embargoed+=1;}
    }
    let enough=caldays.len()>=crate::sniper_preregister::MIN_CAL_DAYS as usize
        &&holddays.len()>=crate::sniper_preregister::MIN_HOLD_DAYS as usize
        &&cal.n>=crate::sniper_preregister::MIN_CAL_LABELS
        &&hold.n>=crate::sniper_preregister::MIN_HOLD_LABELS;
    let basic=json!({"schema":"p1k5g-preregistered-cost-evidence-v1",
        "status":if enough{"descriptive_quote_costs_only"}
                    else{"insufficient_or_unverified"},
        "reason":if enough{"matched_fixed_predeclared_plan"}
                    else{"not_enough_costed_future_sessions"},
        "start_ms":start,"boundary_ms":boundary,"end_exclusive_ms":end,
        "embargo_ms":embargo,"registered_at_freeze_ms":cutoff,
        "calendar_split_fixed_before_future_ticks":true,
        "calibration_days":caldays.len(),"holdout_days":holddays.len(),
        "calibration_labels":cal.n,"holdout_labels":hold.n,
        "embargoed_samples":embargoed,"outside_window_samples":outside,
        "roundtrip_commission_per_lot":commission,
        "hypothetical_slippage_ticks_per_side":slippage,
        "account_currency":protocol["cost_currency"],
        "reference_volume_lots":1.0,
        "costs_include_bid_ask_spread_once":true,
        "cost_method":"midquote_labels_are_not_fills; open_ask_close_bid_buy, open_bid_close_ask_sell",
        "calibration":if enough{cal.json()}else{Value::Null},
        "holdout":if enough{hold.json()}else{Value::Null},
        "baseline_selection_bias":"always_buy_evaluated_only_at_frozen_learner_events",
        "broker_pnl":Value::Null,"strategy_win_rate":Value::Null,
        "independent_oos":false,"broker_fills_observed":false,
        "automatic_promotion":false,"complete_market_tick_coverage":false,
        "note":"Hypothetical 5s exit on future broker quotes per 1.0 reference lot. Fees and slippage are freeze-time assumptions; no executable fills, stop/target, rejection, margin, portfolio risk or true net profit."});
    basic
}
pub fn from_journal(db:&Connection,account:&str,id:&str,hash:&str)->Result<Value>{
    let prior=crate::sniper_forward_journal::report(db,account,id,hash)?;
    ensure!(prior["integrity"]=="verified_local",
        "Invalid or discontinuous prospective events");
    let proof=crate::sniper_research_artifact::verify(db,account,id,Some(hash))?;
    let cutoff=proof["cutoff_ms"].as_u64().unwrap_or(0);
    let samples=crate::sniper_forward_journal::validated_maturities(db,account,id,hash)?;
    Ok(diagnose(&samples,&proof["preregistered_forward_protocol"],cutoff))
}
#[cfg(test)]
mod tests{
    use super::*;
    fn sample(day:u64,min:u64,protocol:&Value)->Value{
        let start=protocol["start_ms"].as_u64().unwrap();
        let t=start+day*86_400_000+60*60_000+min*10_000;
        // LONG: ask=2500.02 -> sell at future bid=2500.20:
        // gross 18 per 1 lot, 7 commission, 4 modeled slip => +7.
        json!({"schema":"p1k5e-frozen-future-event-v1","kind":"matured",
            "opened_broker_ms":t,"closed_broker_ms":t+5000,"side":1,
            "entry_bid":2500.,"entry_ask":2500.02,"exit_bid":2500.2,
            "exit_ask":2500.22,"entry_tick_size":0.01,
            "tick_value_loss_per_lot":1.,"tick_value_profit_per_lot":1.,
            "gross_quote_markout_price":0.18,
            "quote_gross_account_currency_per_lot":18.,
            "modeled_slippage_account_currency_per_lot":4.,
            "commission_roundtrip_account_currency_per_lot":7.,
            "modeled_net_quote_account_currency_per_lot":7.,
            "modeled_only":true,"broker_fill":false,
            "outcome":"correct","midquote_change":0.20,"threshold_tick":0.01})
    }
    #[test]fn registered_calendar_and_spread_fee_tick_values_can_be_replayed(){
        let cfg=braintrade_domain::Config::default();
        let cutoff=1_781_000_000_000;
        let protocol=crate::sniper_preregister::preregister(&cfg,cutoff,"USD");
        let mut samples=vec![];
        for d in [0u64,1,2,3,10,11] {
            for minute in 0..60 {samples.push(sample(d,minute,&protocol))}
        }
        let r=diagnose(&samples,&protocol,cutoff);
        assert_eq!(r["status"],"descriptive_quote_costs_only");
        assert_eq!(r["calibration_days"],4);
        assert_eq!(r["holdout_days"],2);
        assert_eq!(r["holdout_labels"],120);
        assert_eq!(r["holdout"]["mean_modeled_quote_after_cost_per_1_lot"],7.);
        assert_eq!(r["holdout"]["mean_baseline_buy_after_cost_per_1_lot"],7.);
        assert_eq!(r["broker_pnl"],Value::Null);
        assert_eq!(r["independent_oos"],false);
    }
    #[test]fn missing_cost_fields_or_shifted_holdout_is_never_valid(){
        let c=braintrade_domain::Config::default();
        let cutoff=1_781_000_000_000;
        let p=crate::sniper_preregister::preregister(&c,cutoff,"USD");
        let mut samples=vec![];
        for d in [0u64,1,2,3,10,11] {
            for min in 0..60 {samples.push(sample(d,min,&p))}
        }
        samples[0]["exit_bid"]=json!(2500.99);
        let r=diagnose(&samples,&p,cutoff);
        assert_eq!(r["reason"],"missing_broker_tick_value_or_inconsistent_cost_evidence");
        let mut shifted=p.clone();
        shifted["boundary_ms"]=json!(p["boundary_ms"].as_u64().unwrap()+86_400_000);
        assert_eq!(diagnose(&samples,&shifted,cutoff)["reason"],"missing_or_changed_freeze_time_protocol");
        samples[0]=sample(0,0,&p);
        samples[0]["tick_value_loss_per_lot"]=Value::Null;
        assert_eq!(diagnose(&samples,&p,cutoff)["status"],"insufficient_or_unverified");
    }
}
