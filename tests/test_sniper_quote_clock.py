"""P1-K1: distinguish broker time from the logical quote deduplication clock."""
import sys
from pathlib import Path
from types import SimpleNamespace
import unittest

sys.path.insert(0, str(Path(__file__).parents[1] / "gateway"))
import mt5_gateway as gateway


class QuoteClockTests(unittest.TestCase):
    def snapshot(self):
        return dict(connected=True, login=77, server="Demo", currency="USD",
                    account_trade_mode=0, symbol="XAUUSD", bid=2500.2, ask=2500.4,
                    quote_time_ms=5005, source_quote_time_ms=5000, tick_age_ms=1,
                    native_event_id=5000_000001, positions=[])

    def test_poll_does_not_reintroduce_quote_older_than_native(self):
        before = self.snapshot()
        mt5 = SimpleNamespace(symbol_info_tick=lambda _: SimpleNamespace(
            time_msc=4999, bid=2499.8, ask=2500.0))
        after, changed, _ = gateway.refresh_fast_tick(before, mt5, "XAUUSD", now_ms=5002)
        self.assertFalse(changed)
        self.assertEqual((after["bid"], after["source_quote_time_ms"]), (2500.2, 5000))

    def test_native_does_not_reintroduce_quote_older_than_poll(self):
        before = self.snapshot()
        before["native_event_id"] = 4900_000001
        packet = dict(login=77, symbol="XAUUSD", time_msc=4999,
                      event_id=4999_000001, bid=2499.8, ask=2500.0)
        after, changed, _ = gateway.apply_native_tick(before, packet, now_ms=5002)
        self.assertFalse(changed)
        self.assertEqual(after["source_quote_time_ms"], 5000)

    def test_reconciliation_keeps_account_updates_without_rewinding_quote(self):
        before = self.snapshot()
        current = dict(before, quote_time_ms=4999, source_quote_time_ms=4999,
                       bid=2499.8, ask=2500.0, tick_age_ms=5, positions=[{"ticket":22}])
        after, changed = gateway.advance_quote_clock(before, current)
        self.assertFalse(changed)
        self.assertEqual(after["positions"], [{"ticket":22}])
        self.assertEqual((after["bid"], after["quote_time_ms"], after["source_quote_time_ms"]),
                         (2500.2, 5005, 5000))
        self.assertGreaterEqual(after["tick_age_ms"], 5)

    def test_account_change_starts_its_own_quote_clock(self):
        before = self.snapshot()
        after, changed = gateway.advance_quote_clock(before, dict(before,
            login=88, quote_time_ms=4900, source_quote_time_ms=4900, bid=2499.8))
        self.assertTrue(changed)
        self.assertEqual(after["quote_time_ms"], 4900)
        self.assertIsNone(after["native_event_id"])

    def test_pulse_carries_real_broker_time_even_under_dense_same_ms_burst(self):
        state = self.snapshot()
        for i in range(2100):
            candidate = dict(state, bid=2500.2 + i * .01, ask=2500.4 + i * .01)
            state, _ = gateway.advance_quote_clock(state, candidate)
        self.assertGreater(state["quote_time_ms"], 7002)
        self.assertEqual(gateway.pulse_payload(state)["source_quote_time_ms"], 5000)


if __name__ == "__main__":
    unittest.main()
