//! P1-K5G: freeze-time registration of a fixed FUTURE calendar split
//! and broker cost assumptions. The registration is embedded inside the
//! SHA-256 frozen research artifact before any future market label exists.
//! This is descriptive validation, NOT independently certified OOS.
use braintrade_domain::Config;
use serde_json::{json,Value};
const DAY:u64=86_400_000;
const CAL_DAYS:u64=10;
const HOLD_DAYS:u64=10;
pub const EMBARGO_MS:u64=1_800_000;
pub const MIN_CAL_DAYS:u64=4;
pub const MIN_HOLD_DAYS:u64=2;
pub const MIN_CAL_LABELS:u64=50;
pub const MIN_HOLD_LABELS:u64=100;

/// Both the calendar cut and costs are frozen at the model freeze,
/// never chosen after seeing which days have good results.
pub fn preregister(c:&Config,cutoff:u64,currency:&str)->Value{
    let start=(cutoff/DAY).saturating_add(1).saturating_mul(DAY);
    let boundary=start.saturating_add(CAL_DAYS.saturating_mul(DAY));
    let end=boundary.saturating_add(HOLD_DAYS.saturating_mul(DAY));
    json!({"schema":"p1k5g-predeclared-prospective-v1",
        "registered_at_freeze_ms":cutoff,
        "start_ms":start,"boundary_ms":boundary,"end_exclusive_ms":end,
        "calendar_method":"fixed_utc_midnights_pre_observation",
        "calibration_calendar_days":CAL_DAYS,"holdout_calendar_days":HOLD_DAYS,
        "embargo_ms":EMBARGO_MS,
        "min_calibration_session_days":MIN_CAL_DAYS,
        "min_holdout_session_days":MIN_HOLD_DAYS,
        "min_calibration_maturities":MIN_CAL_LABELS,
        "min_holdout_maturities":MIN_HOLD_LABELS,
        "horizon_ms":5000,"baseline":"buy_on_same_learner_selected_events",
        "commission_round_trip_per_lot":c.risk.commission_per_lot,
        "slippage_ticks_per_side":c.risk.slippage_ticks,
        "cost_currency":currency,
        "lot_reference":1.0,
        "costs_method":"bid_ask_open_close_with_tick_value_and_per_lot_fee",
        "no_broker_fills":true,"no_real_pnl":true,
        "independent_oos":false,"automatic_promotion":false
    })
}
pub fn valid(p:&Value,cutoff:u64)->bool{
    let start=(cutoff/DAY).saturating_add(1).saturating_mul(DAY);
    let boundary=start.saturating_add(CAL_DAYS*DAY);
    let end=boundary.saturating_add(HOLD_DAYS*DAY);
    p["schema"]=="p1k5g-predeclared-prospective-v1"
        &&p["registered_at_freeze_ms"].as_u64()==Some(cutoff)
        &&p["start_ms"].as_u64()==Some(start)
        &&p["boundary_ms"].as_u64()==Some(boundary)
        &&p["end_exclusive_ms"].as_u64()==Some(end)
        &&p["horizon_ms"]==5000
        &&p["embargo_ms"].as_u64()==Some(EMBARGO_MS)
        &&p["calibration_calendar_days"].as_u64()==Some(CAL_DAYS)
        &&p["holdout_calendar_days"].as_u64()==Some(HOLD_DAYS)
        &&p["min_calibration_session_days"].as_u64()==Some(MIN_CAL_DAYS)
        &&p["min_holdout_session_days"].as_u64()==Some(MIN_HOLD_DAYS)
        &&p["min_calibration_maturities"].as_u64()==Some(MIN_CAL_LABELS)
        &&p["min_holdout_maturities"].as_u64()==Some(MIN_HOLD_LABELS)
        &&p["calendar_method"]=="fixed_utc_midnights_pre_observation"
        &&p["costs_method"]=="bid_ask_open_close_with_tick_value_and_per_lot_fee"
        &&p["lot_reference"]==1.0
        &&p["no_broker_fills"]==true
        &&p["no_real_pnl"]==true
        &&p["independent_oos"]==false
        &&p["automatic_promotion"]==false
        &&p["commission_round_trip_per_lot"].as_f64()
            .is_some_and(|x|x.is_finite()&&x>=0.)
        &&p["slippage_ticks_per_side"].as_f64()
            .is_some_and(|x|x.is_finite()&&x>=0.)
        &&p["cost_currency"].as_str()
            .is_some_and(|x|!x.is_empty())
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn frozen_calendar_cannot_shift_to_favorable_days(){
        let config=Config::default();
        let c=1_783_000_000_000;
        let p=preregister(&config,c,"USD");
        assert!(valid(&p,c));
        assert!(p["start_ms"].as_u64().unwrap()>c);
        let mut shifted=p.clone();shifted["boundary_ms"]=json!(p["boundary_ms"].as_u64().unwrap()+DAY);
        assert!(!valid(&shifted,c));
        assert!(!valid(&p,c+DAY));
    }
    #[test]fn cost_assumptions_are_locked_into_original_protocol(){
        let c=Config::default();
        let mut p=preregister(&c,1_783_000_000_000,"USD");
        assert_eq!(p["commission_round_trip_per_lot"],c.risk.commission_per_lot);
        assert_eq!(p["slippage_ticks_per_side"],c.risk.slippage_ticks);
        p["commission_round_trip_per_lot"]=json!(-2.0);
        assert!(!valid(&p,1_783_000_000_000));
    }
}
