"""End-to-end display/control dataflow audit against the shipping source.

No mock trade outcome may be confused with booked Demo PnL. Tests deliberately
check wiring between broker snapshots, Core, learning evidence and Slint.
The Rust coordinator closed-deal test asserts the underlying state transition.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]

def read(path):
    return (ROOT / path).read_text(encoding="utf-8")


class FullSystemDataflowAudit(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.gateway = read("gateway/mt5_gateway.py")
        cls.gateway_execution = read("gateway/execution.py")
        cls.core = read("crates/runtime/src/lib.rs")
        cls.engine = read("crates/runtime/src/execution.rs")
        cls.legacy = read("crates/runtime/src/execution_legacy.rs")
        cls.coordinator = read("crates/runtime/src/coordinator_learning.rs")
        cls.sniper = read("crates/runtime/src/sniper_learning.rs")
        cls.pulse = read("crates/runtime/src/execution_pulse.rs")
        cls.student = read("crates/desktop/src/student.rs")
        cls.desktop = read("crates/desktop/src/main.rs")
        cls.execution = read("crates/desktop/src/execution.rs")
        cls.studio = read("crates/desktop/src/studio.rs")
        cls.ui = read("crates/desktop/ui/main.slint")

    def test_broker_snapshots_to_current_dashboard_never_display_stale_or_disconnected_account(self):
        self.assertIn("mt5.positions_get()", self.gateway)
        self.assertIn("mt5.orders_get()", self.gateway)
        self.assertIn("mt5.history_deals_get(start, end)", self.gateway)
        self.assertIn("copy_rates_from_pos(symbol,getattr(mt5,constant),1,180)", self.gateway)
        self.assertIn("let broker_online = status.gateway_fresh", self.desktop)
        self.assertIn("status.telemetry.as_ref().is_some_and(|t| t.connected)", self.desktop)
        self.assertIn("ui.set_online(broker_online);", self.desktop)
        self.assertIn("status.telemetry.filter(|_| broker_online)", self.desktop)
        for field in ("account", "balance", "equity", "today", "positions", "connection", "diagnostics"):
            self.assertIn(f"ui.set_{field}(", self.desktop)
            self.assertIn(f"root.{field}", self.ui)

    def test_normal_sniper_demo_trade_reaches_actual_specialist_model(self):
        self.assertIn("sniper_learning::refresh(&mut self.inner.sniper", self.engine)
        self.assertIn('owner == "system"', self.coordinator)
        self.assertIn('selected == Some(braintrade_domain::sniper::ID)', self.coordinator)
        self.assertIn('coordination["selection"]["policy"] == "sniper-ticks-v1"', self.coordinator)
        self.assertIn("mt5_demo_broker", self.coordinator)
        self.assertIn("sniper_tp_preserved", self.coordinator)
        self.assertIn("fn coordinator_owned_sniper_close_trains_the_specialist_from_exact_demo_deals_once()", self.coordinator)
        self.assertIn("brain='sniper_learner'", self.sniper)
        self.assertIn('s["sniper_target_mode"]=="adaptive"', self.sniper)
        self.assertIn('p["sniper_tp_preserved"]==true', self.sniper)
        self.assertIn("ORDER BY s.closed_ms DESC", self.sniper)
        self.assertIn("LEFT JOIN coordinator_broker_admissions a", self.sniper)
        self.assertIn("verified_sniper_admission(&row,&context,now)", self.sniper)
        self.assertIn("admitted>now", self.sniper)
        self.assertIn("payload_sha256", self.sniper)
        self.assertIn("LIMIT 2000", self.sniper)

    def test_executable_quote_fast_path_does_not_replace_identity_or_reconciliation(self):
        for item in ("pulse.login == self.telemetry.login", "pulse.server == self.telemetry.server",
                     "pulse.currency == self.telemetry.currency", "pulse.symbol == self.telemetry.symbol",
                     "pulse.quote_time_ms > previous"):
            self.assertIn(item, self.pulse)
        self.assertIn("tick_age_ms", self.gateway)
        self.assertIn("tick_age_ms", self.pulse)
        for item in ("max_quote_age_ms", "order_send"):
            self.assertIn(item, self.gateway_execution)
        self.assertIn("self.student.observe(db,telemetry,now)?;", self.engine)
        self.assertIn("coordinator_learning::available_frames", self.legacy)
        self.assertIn("let sniper_signal = self.sniper.evaluate(t,c,now);", self.legacy)

    def test_brain_switch_requires_loaded_settings_and_one_consistent_identity(self):
        self.assertIn("exec-settings-loaded: false", self.ui)
        self.assertIn("enabled: !root.exec-busy; selected(value)", self.ui)
        self.assertIn("ui.set_exec_settings_loaded(true)", self.execution)
        self.assertIn('matches!(action.as_str(), "select" | "refresh")', self.execution)
        self.assertIn("ui.set_exec_settings_loaded(false)", self.execution)
        self.assertGreaterEqual(self.ui.count("root.exec-settings-loaded"), 4)
        self.assertIn('root.execution-action("enable")', self.ui)
        self.assertIn('root.execution-action("disable")', self.ui)
        self.assertIn('root.execution-action("close")', self.ui)
        self.assertIn("ui.get_exec_selected()", self.execution)

    def test_roundtrip_risk_fee_is_distinct_from_per_side_trading_fee(self):
        self.assertIn('risk-commission-lot: "7"', self.ui)
        self.assertIn("text <=> root.risk-commission-lot", self.ui)
        self.assertIn("text <=> root.commission-lot", self.ui)
        self.assertIn("ui.get_risk_commission_lot()", self.desktop)
        self.assertNotIn("ui.get_commission_lot(),\n                ui.get_slippage_ticks()", self.desktop)
        self.assertIn("ui.set_risk_commission_lot(", self.desktop)
        self.assertIn("ui.set_risk_commission_lot(roundtrip.to_string().into())", self.studio)
        self.assertIn('"commission_basis":basis', self.studio)
        self.assertIn('root.state == "STOPPED"; clicked => { root.save-risk(); }', self.ui)
        self.assertIn('root.state == "STOPPED" && (root.exec-target == 0 || root.exec-confirm-login != "")', self.ui)
        self.assertIn('clicked => { root.execution-action("bind"); }', self.ui)
        self.assertIn('confirm_live_login', self.execution)

    def test_learning_dashboard_only_describes_observed_or_research_specific_data(self):
        self.assertIn('/v1/execution/student', self.student)
        self.assertIn("closed_broker_trades", self.student)
        self.assertIn("usable_shadow_pairs", self.student)
        self.assertIn("recent_pattern_counts", self.student)
        self.assertIn("observed_mfe_price", self.student)
        self.assertIn("shadow", self.student)
        self.assertIn('root.student-policies', self.ui)
        self.assertIn('root.student-journal', self.ui)
        self.assertIn('root.exec-detail', self.ui)
        self.assertIn('root.exec-ledger', self.ui)
        self.assertIn("ui.set_exec_ledger(", self.execution)
        self.assertIn("ui.set_exec_detail(", self.execution)
        self.assertIn("execution::magic(&b.id)", self.legacy)

if __name__ == "__main__":
    unittest.main()
