from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SHADOW = (ROOT / "crates/runtime/src/shadow_exit.rs").read_text(encoding="utf-8")
EXECUTION = (ROOT / "crates/runtime/src/execution.rs").read_text(encoding="utf-8")


class ShadowAdaptiveExitContract(unittest.TestCase):
    def test_shadow_module_has_no_broker_command_writer(self):
        self.assertNotIn("INSERT INTO broker_commands", SHADOW)
        self.assertNotIn("UPDATE broker_commands", SHADOW)
        self.assertNotIn("/v1/execution/close", SHADOW)
        self.assertIn('"can_send_orders":false', SHADOW)

    def test_shadow_is_demo_scoped_and_uses_executable_exit_side(self):
        self.assertIn("!t.demo||t.account_trade_mode", SHADOW.replace(" ", ""))
        self.assertIn('"BUY" => t.bid', SHADOW)
        self.assertIn('"SELL" => t.ask', SHADOW)

    def test_exact_actual_pnl_includes_all_booked_cost_fields(self):
        self.assertIn('deal["profit"]', SHADOW)
        self.assertIn('deal["commission"]', SHADOW)
        self.assertIn('deal["swap"]', SHADOW)
        self.assertIn('deal["fee"]', SHADOW)
        self.assertIn("exact_booked_mt5_deals_profit_plus_commission_plus_swap_plus_fee", SHADOW)

    def test_fast_pulse_does_not_call_shadow_observer(self):
        start = EXECUTION.index('if method == "PUT" && path == "/v1/execution/pulse"')
        end = EXECUTION.index('if method == "POST" && path == "/v1/execution/portfolio"', start)
        pulse = EXECUTION[start:end]
        self.assertNotIn("shadow_exit::observe", pulse)
        self.assertIn("shadow_exit::observe", EXECUTION[:start])

    def test_shadow_endpoint_is_read_only(self):
        self.assertIn('method == "GET" && path == "/v1/execution/shadow-exit"', EXECUTION)
        self.assertNotIn('method == "POST" && path == "/v1/execution/shadow-exit"', EXECUTION)


if __name__ == "__main__":
    unittest.main()
