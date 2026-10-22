//! Strict composition of two already isolated synthetic research models.
//! A lifecycle order must first be hypothetically eligible in the admission
//! scenario, for the same synthetic account, symbol and *exact* lot request.
//! This bridge neither places broker orders nor reads any real account data.
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MAX_BYTES: usize = 2_000_000;
const EPS: f64 = 1e-8;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    admission: Value,
    lifecycle: Value,
}
#[derive(Clone, Copy)]
struct Entry {
    lots: f64,
    eligible: bool,
}

pub fn compare(body: &[u8]) -> Result<Value> {
    ensure!(body.len() <= MAX_BYTES, "Combined research scenario is too large");
    let scenario: Scenario = serde_json::from_slice(body)?;
    // Both engines enforce their own bounded, strictly validated contracts.
    // Neither returns a partial result for a malformed/contradictory scenario.
    let admission = crate::research_admission::evaluate(
        serde_json::to_vec(&scenario.admission)?.as_slice()
    ).context("Independent hypothetical admission failed")?;
    let lifecycle = crate::research_order_lifecycle::replay(
        serde_json::to_vec(&scenario.lifecycle)?.as_slice()
    ).context("Independent synthetic order-event replay failed")?;
    let account = admission["account"].as_str().context("Missing synthetic admission account")?;
    let symbol = admission["symbol"].as_str().context("Missing synthetic admission symbol")?;
    ensure!(
        lifecycle["account"].as_str() == Some(account)
        && lifecycle["symbol"].as_str() == Some(symbol),
        "Synthetic account or symbol mismatch between admission and order lifecycle"
    );
    let mut candidates = BTreeMap::<String, Entry>::new();
    for decision in admission["decisions"].as_array().context("Missing admission decisions")? {
        let id = decision["id"].as_str().context("Admission candidate has no ID")?;
        let lots = decision["requested_lots"].as_f64().context("Missing candidate lots")?;
        let eligible = decision["eligible_hypothetical"].as_bool()
            .context("Missing synthetic admission eligibility")?;
        ensure!(lots.is_finite() && lots > 0., "Invalid candidate lots");
        ensure!(candidates.insert(id.to_string(), Entry { lots, eligible }).is_none(),
            "Duplicate admission candidate identity");
    }
    let mut accepted_submissions = 0usize;
    let mut submitted_lots = 0.0;
    let mut submitted_ids = Vec::<String>::new();
    for order in lifecycle["orders"].as_array().context("Missing lifecycle orders")? {
        let id = order["id"].as_str().context("Missing lifecycle order identity")?;
        let lots = order["requested_lots"].as_f64().context("Missing lifecycle requested lot")?;
        let admission_entry = candidates.get(id)
            .ok_or_else(|| anyhow::anyhow!("Order {id} has no admission decision"))?;
        ensure!(admission_entry.eligible,
            "Order {id} failed synthetic admission and cannot enter lifecycle replay");
        ensure!(lots.is_finite() && lots > 0.
            && (lots-admission_entry.lots).abs() <= EPS,
            "Order {id} requested lot differs from independently admitted lot");
        accepted_submissions += 1;
        submitted_lots += lots;
        submitted_ids.push(id.to_string());
    }
    let eligible_count = candidates.values().filter(|c| c.eligible).count();
    ensure!(accepted_submissions <= eligible_count,
        "Lifecycle contains more orders than admitted candidates");
    let not_submitted = eligible_count-accepted_submissions;
    Ok(json!({
        "status":"synthetic_only",
        "policy":"admission-to-lifecycle-offline-v1",
        "account":account,
        "symbol":symbol,
        "admitted_candidate_count":eligible_count,
        "blocked_candidate_count":admission["blocked"],
        "submitted_after_admission":accepted_submissions,
        "eligible_but_not_submitted":not_submitted,
        "submitted_lots":submitted_lots,
        "submitted_ids":submitted_ids,
        "hypothetical_admission":admission,
        "synthetic_order_lifecycle":lifecycle,
        "mt5_commands":0,
        "broker_authorizations":0,
        "broker_fills_inferred":0,
        "no_learning_updates":true,
        "no_persistent_mutation":true,
        "caveat":"Scenario-only conservative composition. Eligibility is not order_check or an MT5 permission. Admission reserves all accepted candidates even if not submitted. Lifecycle changes only on explicit synthetic fill/close events; no real prices, fills, risk model equivalence or broker PnL."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admission() -> Value {
        json!({
            "account":"demo_77","symbol":"XAUUSD",
            "quote":{"bid":5000.0,"ask":5000.2,"age_ms":30},
            "contract":{"volume_min":0.01,"volume_max":2.0,
                "volume_step":0.01,"tick_size":0.1,"tick_value_loss":1.0,
                "estimated_margin_per_lot":100.0},
            "limits":{"equity":500.0,"free_margin":100.0,
                "max_positions":1,"max_total_lots":0.1,"max_stop_risk_percent":2.0,
                "commission_roundtrip_per_lot":7.0,"slippage_ticks_per_side":1.0},
            "existing":[],
            "requests":[
                {"id":"A","side":"buy","lots":0.04,"stop_price":4999.2},
                {"id":"B","side":"sell","lots":0.04,"stop_price":5000.8}
            ]
        })
    }
    fn life(events: Value) -> Value {
        json!({"account":"demo_77","symbol":"XAUUSD","events":events})
    }
    fn eval(a:Value,l:Value) -> Result<Value>{
        compare(json!({"admission":a,"lifecycle":l}).to_string().as_bytes())
    }
    #[test]
    fn only_one_hypothetically_admitted_order_can_enter_execution_lifecycle() {
        let events=json!([
            {"event":"submit","id":"A","lots":0.04},
            {"event":"dispatch","id":"A"},
            {"event":"unknown","id":"A"}
        ]);
        let result=eval(admission(),life(events)).unwrap();
        assert_eq!(result["admitted_candidate_count"],1);
        assert_eq!(result["blocked_candidate_count"],1);
        assert_eq!(result["submitted_after_admission"],1);
        assert_eq!(result["synthetic_order_lifecycle"]["orders"][0]["state"],"unknown");
        assert_eq!(result["synthetic_order_lifecycle"]["orders"][0]["filled_lots"],0.);
        assert_eq!(result["broker_fills_inferred"],0);
        assert_eq!(result["mt5_commands"],0);
    }
    #[test]
    fn an_entry_denied_by_the_risk_budget_must_not_be_submitted() {
        let events=json!([{"event":"submit","id":"B","lots":0.04}]);
        assert!(eval(admission(),life(events)).is_err());
    }
    #[test]
    fn account_symbol_and_lot_mismatches_are_refused() {
        let bad_lot=json!([{"event":"submit","id":"A","lots":0.05}]);
        assert!(eval(admission(),life(bad_lot)).is_err());
        let l=life(json!([{"event":"submit","id":"A","lots":0.04}]));
        let mut wrong=l.clone();
        wrong["symbol"]=json!("EURUSD");
        assert!(eval(admission(),wrong).is_err());
        let mut wrong=l;
        wrong["account"]=json!("demo_88");
        assert!(eval(admission(),wrong).is_err());
    }
    #[test]
    fn only_explicit_idempotent_deals_can_modify_synthetic_position_volume() {
        let events=json!([
            {"event":"submit","id":"A","lots":0.04},
            {"event":"dispatch","id":"A"},
            {"event":"unknown","id":"A"},
            {"event":"fill","id":"A","deal_id":"f1","lots":0.02},
            {"event":"fill","id":"A","deal_id":"f1","lots":0.02},
            {"event":"reject_remaining","id":"A"},
            {"event":"close","id":"A","deal_id":"c1","lots":0.02}
        ]);
        let result=eval(admission(),life(events)).unwrap();
        assert_eq!(result["synthetic_order_lifecycle"]["orders"][0]["state"],
            "partial_terminal_flat");
        assert_eq!(result["synthetic_order_lifecycle"]["idempotent_replays"],1);
        assert_eq!(result["synthetic_order_lifecycle"]["open_lots"],0.);
        assert_eq!(result["synthetic_order_lifecycle"]["unresolved_dispatched_entries"],0);
        assert!(result["synthetic_order_lifecycle"]["profit_loss"].is_null());
    }
    #[test]
    fn allowed_but_never_submitted_is_not_claimed_as_a_broker_fill() {
        let result=eval(admission(),life(json!([]))).unwrap();
        assert_eq!(result["admitted_candidate_count"],1);
        assert_eq!(result["eligible_but_not_submitted"],1);
        assert_eq!(result["submitted_after_admission"],0);
        assert_eq!(result["synthetic_order_lifecycle"]["open_lots"],0.);
    }
    #[test]
    fn malformed_scenario_does_not_bypass_both_independent_validations() {
        let mut a=admission();
        a["send_live"]=json!(true);
        assert!(eval(a,life(json!([]))).is_err());
        let mut invalid=life(json!([]));
        invalid["order_send"]=json!(true);
        assert!(eval(admission(),invalid).is_err());
    }
}
