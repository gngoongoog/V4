from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


class DemoRealTrainingContractTests(unittest.TestCase):
    def text(self, path: str) -> str:
        return (ROOT / path).read_text(encoding="utf-8")

    def test_demo_execution_reaches_real_mt5_order_send_after_durable_journal(self):
        source = self.text("gateway/execution.py")
        journal = source.index("INSERT INTO sends(id,request,result,created_ms)")
        commit = source.index("self.db.commit()", journal)
        send = source.index("mt5.order_send(request)")
        self.assertLess(journal, commit)
        self.assertLess(commit, send)
        self.assertIn("target_mode = 0 if cmd['target']=='demo'", source)
        self.assertIn("Bound account or Demo/Live mode changed", source)

    def test_optional_entry_limits_are_off_by_default(self):
        wrapper = self.text("crates/runtime/src/execution.rs")
        brain = self.text("crates/domain/src/execution_legacy.rs")
        self.assertIn("enabled: false", wrapper)
        self.assertIn("use_risk_limits: false", brain)
        self.assertIn("if b.use_risk_limits", self.text("crates/runtime/src/execution_legacy.rs"))

    def test_micro_risk_rejection_is_independent_in_core_and_broker(self):
        core = self.text("crates/runtime/src/execution_legacy.rs")
        broker = self.text("gateway/execution.py")
        self.assertIn('c.risk.risk_percent > 2.0', core)
        self.assertIn('self.settings.target != "demo"', core)
        self.assertIn('t.account_trade_mode != Some(0)', core)
        self.assertIn("micro_demo_risk_opt_in=cmd.get('micro_demo_risk_opt_in') is True", broker)
        self.assertIn("broker_demo=cmd['target']=='demo' and a.trade_mode==0", broker)
        self.assertIn("20.0 if micro_real_active", broker)
        self.assertIn("5.0 if micro_demo_risk_opt_in and broker_demo", broker)
        self.assertIn("else 2.0", broker)
        self.assertIn("micro_real_recovered", broker)

    def test_coordinator_is_not_shadow_locked_and_start_remains_explicit(self):
        wrapper = self.text("crates/runtime/src/execution.rs")
        core = self.text("crates/runtime/src/lib.rs")
        self.assertIn("system.enabled = true", wrapper)
        self.assertIn("status.requested_run && status.gateway_fresh && self.account_matches(status)", wrapper)
        self.assertIn("منسّق الوضع المختار وحده يرسل أوامر إلى الحساب المرتبط الحديث", core)
        self.assertIn('"broker_execution_possible":true', core)
        self.assertIn('"requires_fresh_bound_account":true', core)
        self.assertNotIn('result["start"]==true', core)
        self.assertNotIn('json!({"execution":false})', core)

    def test_real_demo_learning_uses_exact_booked_broker_costs(self):
        source = self.text("crates/runtime/src/coordinator_learning.rs")
        compact = "".join(source.split())
        self.assertIn("coordinator_broker_samples", source)
        self.assertIn('"mt5_demo_broker"', source)
        # Coordinator delegates exact broker-cost attestation to the same
        # journal validator used by the operator's true MT5 trade history.
        # Literal inline field sums allowed missing fees to turn into $0.
        receipts = self.text("crates/runtime/src/trade_journal.rs")
        self.assertIn('crate::trade_journal::booked_costs(deals.iter())', source)
        self.assertIn('crate::trade_journal::fully_closed_by_deals(deals.iter())', source)
        for field in ('"profit"', '"commission"', '"swap"', '"fee"'):
            self.assertIn(field, receipts)
        self.assertIn('d[*name].as_f64().filter(|n| n.is_finite())?', receipts)
        self.assertIn("if closed_ms==0", source)
        self.assertNotIn('d["commission"].as_f64().unwrap_or(0.)', source)
        self.assertNotIn('d["fee"].as_f64().unwrap_or(0.)', source)
        self.assertIn("!t.demo||t.account_trade_mode!=Some(0)", compact)
        self.assertIn('member["ready"]!=true', compact)
        # Intelligence V3 names the agreement explicitly so the same value can also be
        # persisted in the counterfactual ledger. Exact broker evidence must still be
        # credited only to members that supported the executed coordinator side.
        self.assertIn('letsupported=member["side"].as_i64()==Some(executed_side)', compact)
        self.assertIn("if!supported{continue;}", compact)
        self.assertIn("supported_execution", source)
        self.assertIn('"costs":"exact booked MT5 deal profit + commission + swap + fee"', source)


if __name__ == "__main__":
    unittest.main()
