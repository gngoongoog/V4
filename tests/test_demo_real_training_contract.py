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

    def test_coordinator_is_not_shadow_locked(self):
        wrapper = self.text("crates/runtime/src/execution.rs")
        core = self.text("crates/runtime/src/lib.rs")
        self.assertIn("system.enabled = true", wrapper)
        self.assertIn("status.requested_run && status.gateway_fresh && self.account_matches(status)", wrapper)
        self.assertIn("Enabled independent brains may send broker orders to the bound account", core)

    def test_real_demo_learning_uses_exact_booked_broker_costs(self):
        source = self.text("crates/runtime/src/coordinator_learning.rs")
        self.assertIn("coordinator_broker_samples", source)
        self.assertIn('"mt5_demo_broker"', source)
        for field in ('d["profit"]', 'd["commission"]', 'd["swap"]', 'd["fee"]'):
            self.assertIn(field, source)
        self.assertIn("t.demo||t.account_trade_mode!=Some(0)", source)
        self.assertIn("member[\"ready\"]!=true", source)
        self.assertIn("member[\"side\"].as_i64()!=Some(executed_side)", source)


if __name__ == "__main__":
    unittest.main()
