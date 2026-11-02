//! Broker-lot geometry for a prospective TP1/TP2/TP3 partial-exit manager.
//!
//! Read-only. No order commands, policy mutations, or promises of broker fills.
//! The future executor must separately handle account mode, position ownership,
//! live volume/SL/TP, prices, stops/freeze levels and durable reconciliation.
use serde::Serialize;
use serde_json::{json, Value};
use crate::{execution, Telemetry};

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ThreeTargetLots {
    pub tp1: f64,
    pub tp2: f64,
    pub tp3: f64,
    pub remaining_after_tp1: f64,
    pub remaining_after_tp2: f64,
    pub remaining_after_tp3: f64,
}

fn exact_steps(lots: f64, step: f64) -> Option<u64> {
    if !lots.is_finite() || !step.is_finite() || lots <= 0. || step <= 0. {
        return None;
    }
    let steps = lots / step;
    if !steps.is_finite() || steps < 1. || steps > 1_000_000_000. {
        return None;
    }
    let rounded = steps.round();
    // Avoid optimistic rounding to a broker-lot grid we cannot attest.
    if (rounded - steps).abs() > 1e-7_f64.max(rounded.abs() * 1e-10) {
        return None;
    }
    Some(rounded as u64)
}

/// Fixed *illustrative* 50/30/20 weights. The final leg closes the
/// remainder. Every exit and nonzero residual must meet broker min/step.
/// Volume max is checked conservatively for the observed position.
pub fn three_target_lots(volume: f64, min: f64, step: f64, max: f64)
    -> Result<ThreeTargetLots, &'static str>
{
    if ![volume, min, step, max].iter().all(|v| v.is_finite() && *v > 0.) {
        return Err("missing_or_invalid_broker_volume_rules");
    }
    if min > max || step > max || volume > max + step * 1e-7 {
        return Err("volume_exceeds_reported_broker_range");
    }
    let minimum = exact_steps(min, step).ok_or("unverified_minimum_volume_grid")?;
    let maximum = exact_steps(max, step).ok_or("unverified_maximum_volume_grid")?;
    let total = exact_steps(volume, step).ok_or("position_volume_off_broker_step")?;
    if total > maximum || total < minimum {
        return Err("position_volume_outside_broker_grid");
    }
    if total < minimum.checked_mul(3).ok_or("volume_unit_overflow")? {
        return Err("too_small_for_three_broker_valid_exits");
    }
    // Use integer grid units from now on: never round a partial close volume
    // up to an order larger than the position or leave a dust residual.
    let first = (total * 50 / 100).clamp(minimum, total - minimum * 2);
    let second = (total * 30 / 100).clamp(minimum, total - first - minimum);
    let third = total - first - second;
    if third < minimum {
        return Err("untradeable_final_residual");
    }
    let lots = |units: u64| units as f64 * step;
    Ok(ThreeTargetLots {
        tp1:lots(first), tp2:lots(second), tp3:lots(third),
        remaining_after_tp1:lots(total-first),
        remaining_after_tp2:lots(third),
        remaining_after_tp3:0.,
    })
}

/// Display-only preview tied to one telemetry snapshot; NO inferred TP
/// prices, fills, or authorization of partial closes.
pub fn preview(t: &Telemetry) -> Value {
    let bound = t.connected && t.login > 0 && !t.server.is_empty() &&
        !t.symbol.is_empty() && !t.currency.is_empty() &&
        matches!((t.demo,t.account_trade_mode),(true,Some(0))|(false,Some(2)));
    if !bound {
        return json!({"schema":"partial-exit-volume-preview-v1",
            "status":"unverified_account","read_only":true,
            "orders_enabled":false,"rows":[]});
    }
    let mut rows = Vec::new();
    for p in t.positions.iter().filter(|p|p.symbol==t.symbol
        &&p.ticket>0 &&p.identifier>0 &&p.volume.is_finite()&&p.volume>0.
        &&matches!(p.side.as_str(),"BUY"|"SELL")
        &&execution::brain_for_magic(p.magic).is_some()).take(25) {
        let computed = match (t.volume_min,t.volume_step,t.volume_max) {
            (Some(min),Some(step),Some(max)) =>
                three_target_lots(p.volume,min,step,max),
            _ => Err("broker_volume_rules_unavailable"),
        };
        match computed {
            Ok(lots) => rows.push(json!({
                "position":p.ticket,"position_identifier":p.identifier,
                "position_volume":p.volume,"state":"geometry_valid",
                "lots":lots,"target_weights_percent":[50,30,20],
                "requires_broker_acceptance":true
            })),
            Err(reason) => rows.push(json!({
                "position":p.ticket,"position_identifier":p.identifier,
                "position_volume":p.volume,"state":"unavailable",
                "reason":reason,"lots":Value::Null
            })),
        }
    }
    json!({
        "schema":"partial-exit-volume-preview-v1",
        "status":"read_only_geometry","scope":"owned_positions_current_broker_account_symbol",
        "context":serde_json::to_string(&(&t.server,t.login,&t.symbol,&t.currency))
            .unwrap_or_default(),
        "target":if t.demo{"demo"}else{"live"},
        "read_only":true,"orders_enabled":false,
        "weights_percent":[50,30,20],
        "volume_min":t.volume_min,"volume_step":t.volume_step,"volume_max":t.volume_max,
        "rows":rows,
        "limitations":"Only lot-grid mathematics, not partial-close implementation. TP price/stop levels, trade permissions, hedging/netting, fill policy, freeze limits, costs, and durable reconciliation are not authorized. No broker orders created."
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Position;
    fn approx(a:f64,b:f64){assert!((a-b).abs()<1e-9,"{a} != {b}");}

    #[test]
    fn normal_gold_lots_are_broker_step_valid_and_conserve_volume() {
        let x=three_target_lots(0.10,0.01,0.01,100.).unwrap();
        approx(x.tp1,0.05);approx(x.tp2,0.03);approx(x.tp3,0.02);
        approx(x.remaining_after_tp1,0.05);
        approx(x.remaining_after_tp2,0.02);
        approx(x.tp1+x.tp2+x.tp3,0.10);
        let tiny=three_target_lots(0.03,0.01,0.01,100.).unwrap();
        approx(tiny.tp1,0.01);approx(tiny.tp2,0.01);approx(tiny.tp3,0.01);
        let uneven=three_target_lots(0.11,0.01,0.01,100.).unwrap();
        approx(uneven.tp1+uneven.tp2+uneven.tp3,0.11);
    }

    #[test]
    fn small_volume_and_off_grid_fail_without_fabricating_tradeable_lots() {
        for amount in [0.01,0.02]{
            assert_eq!(three_target_lots(amount,0.01,0.01,100.).unwrap_err(),
                "too_small_for_three_broker_valid_exits");
        }
        assert!(three_target_lots(0.105,0.01,0.01,100.).is_err());
        assert!(three_target_lots(0.10,0.015,0.01,100.).is_err());
        assert!(three_target_lots(0.10,0.01,0.01,0.05).is_err());
        for amount in [f64::NAN,f64::INFINITY,0.,-0.01] {
            assert!(three_target_lots(amount,0.01,0.01,100.).is_err());
        }
        assert!(three_target_lots(0.1,0.01,0.,100.).is_err());
        assert!(three_target_lots(0.1,0.01,0.01,f64::NAN).is_err());
        assert!(three_target_lots(0.1,0.01,0.01,100.005).is_err());
    }

    #[test]
    fn preview_is_read_only_and_filters_foreign_positions_and_unbound_accounts() {
        let owned=Position{ticket:7,identifier:70,magic:410601,symbol:"XAUUSDm".into(),
            side:"BUY".into(),volume:0.1,..Default::default()};
        let t=Telemetry{connected:true,login:10,server:"DemoSrv".into(),
            currency:"USD".into(),symbol:"XAUUSDm".into(),demo:true,
            account_trade_mode:Some(0),volume_min:Some(0.01),
            volume_step:Some(0.01),volume_max:Some(100.),
            positions:vec![owned.clone(),
                Position{magic:99999,ticket:9,..owned.clone()},
                Position{symbol:"EURUSD".into(),ticket:11,..owned.clone()}],
            ..Default::default()};
        let p=preview(&t);
        assert_eq!(p["orders_enabled"],false);
        assert_eq!(p["rows"].as_array().unwrap().len(),1);
        assert_eq!(p["rows"][0]["position"],7);
        assert_eq!(p["rows"][0]["lots"]["tp1"],0.05);
        let no_spec=preview(&Telemetry{volume_min:None,..t.clone()});
        assert_eq!(no_spec["rows"][0]["state"],"unavailable");
        assert_eq!(no_spec["rows"][0]["reason"],"broker_volume_rules_unavailable");
        let unbound=preview(&Telemetry{connected:false,..t});
        assert_eq!(unbound["status"],"unverified_account");
        assert!(unbound["rows"].as_array().unwrap().is_empty());
    }
}
