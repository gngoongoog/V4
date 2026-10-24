"""P0-E synthetic Native/ Poll session admission and MQL5 EA lifecycle.

No MT5 import, no broker order submission and no claim of MetaEditor compile.
"""
from pathlib import Path
from types import SimpleNamespace
import sys
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/"gateway"))
import mt5_gateway as gateway

class MockMT5:
    def __init__(self,login=17,server="Demo-Server",currency="USD",trade_mode=0,connected=True):
        self.account=SimpleNamespace(login=login,server=server,currency=currency,trade_mode=trade_mode)
        self.terminal=SimpleNamespace(connected=connected)
    def account_info(self):
        return self.account
    def terminal_info(self):
        return self.terminal

class NativeSessionGuardTests(unittest.TestCase):
    def snapshot(self):
        return dict(connected=True,login=17,server="Demo-Server",currency="USD",
                    account_trade_mode=0,symbol="XAUUSD",bid=2500.,ask=2500.2)

    def test_verified_current_demo_allows_native_and_poll_pulse_admission(self):
        self.assertTrue(gateway.broker_snapshot_identity_is_current(MockMT5(),self.snapshot()))

    def test_broker_switch_disconnect_and_stale_context_deny_hot_pulse(self):
        cases=[
            MockMT5(login=18),MockMT5(server="Other"),
            MockMT5(currency="EUR"),MockMT5(trade_mode=2),
            MockMT5(connected=False)]
        for mt5 in cases:
            with self.subTest(actual=mt5.account, connected=mt5.terminal.connected):
                self.assertFalse(gateway.broker_snapshot_identity_is_current(mt5,self.snapshot()))
        for snap in (None,{},dict(self.snapshot(),connected=False)):
            self.assertFalse(gateway.broker_snapshot_identity_is_current(MockMT5(),snap))
        bad=MockMT5()
        bad.account=None
        self.assertFalse(gateway.broker_snapshot_identity_is_current(bad,self.snapshot()))
        bad.terminal=None
        self.assertFalse(gateway.broker_snapshot_identity_is_current(bad,self.snapshot()))

    def test_native_hot_path_checks_broker_before_sending_any_pulse(self):
        source=(ROOT/"gateway/mt5_gateway.py").read_text("utf-8")
        native=source.index("packets=native.drain(")
        guard=source.index("if packets and not broker_snapshot_identity_is_current",native)
        send=source.index("work=api('/v1/execution/pulse'",native)
        self.assertLess(native,guard)
        self.assertLess(guard,send)
        fallback=source.index("fast_snapshot,changed,_=refresh_fast_tick")
        guard=source.index("if not broker_snapshot_identity_is_current",fallback)
        send=source.index("work=api('/v1/execution/pulse'",fallback)
        self.assertLess(fallback,guard)
        self.assertLess(guard,send)
        self.assertIn("next_full_snapshot=0.0",source)

    def test_mql5_native_cursor_and_token_rotate_with_account_without_trading(self):
        source=(ROOT/"mt5/BrainTradeSniperBridge.mq5").read_text("utf-8")
        for token in ("void ResetQuoteCursor()", "ACCOUNT_LOGIN", "ACCOUNT_SERVER",
                      "ACCOUNT_CURRENCY", "ACCOUNT_TRADE_MODE",
                      "g_connected_token != g_token",
                      "ResetQuoteCursor();", "ClosePipe();", "g_account_login = login",
                      "g_account_server = server", "g_account_currency = currency",
                      "g_account_mode = mode", "CopyTicks(", "FileFlush(g_pipe)"):
            self.assertIn(token,source)
        self.assertLess(source.index("g_account_login > 0 &&"),source.index("if(g_pipe != INVALID_HANDLE)\n      return true;",source.index("bool EnsurePipe()")))
        for illegal in ("OrderSend(", "OrderSendAsync(", "CTrade", "trade.Buy", "trade.Sell"):
            self.assertNotIn(illegal,source,"Transport-only EA must not trade")

if __name__=="__main__":
    unittest.main()
