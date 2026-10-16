import importlib.util
from pathlib import Path
from types import SimpleNamespace as NS
import unittest

ROOT = Path(__file__).parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


gateway = load("sniper_gateway", ROOT / "gateway" / "mt5_gateway.py")
execution = load("sniper_execution", ROOT / "gateway" / "execution.py")


class TickSource:
    def __init__(self, tick):
        self.tick = tick

    def symbol_info_tick(self, symbol):
        return self.tick


class SniperFastLaneTests(unittest.TestCase):
    def test_sniper_loop_targets_single_digit_milliseconds(self):
        self.assertLessEqual(gateway.SNIPER_LOOP_SECONDS, 0.005)
        self.assertGreaterEqual(gateway.SNIPER_FULL_SNAPSHOT_SECONDS, 0.1)
        self.assertGreater(gateway.SNIPER_BACKGROUND_SECONDS, gateway.SNIPER_LOOP_SECONDS)

    def test_sniper_mode_detection_is_case_insensitive(self):
        self.assertTrue(gateway.sniper_enabled({"trading": {"mode": "Sniper"}}))
        self.assertTrue(gateway.sniper_enabled({"trading": {"mode": "sniper"}}))
        self.assertFalse(gateway.sniper_enabled({"trading": {"mode": "Scalping"}}))
        self.assertFalse(gateway.sniper_enabled({}))

    def test_fast_tick_updates_only_quote_fields(self):
        snapshot = {
            "connected": True,
            "login": 77,
            "server": "DEMO",
            "symbol": "XAUUSD",
            "bid": 2500.0,
            "ask": 2500.2,
            "quote_time_ms": 1_000,
            "tick_age_ms": 0,
            "positions": [{"ticket": 1}],
            "market": {"m1": [1], "m5": [2], "m15": [3]},
        }
        source = TickSource(NS(bid=2500.1, ask=2500.3, time_msc=2_000))
        updated, changed, signature = gateway.refresh_fast_tick(snapshot, source, "XAUUSD", now_ms=2_004)
        self.assertTrue(changed)
        self.assertEqual(signature, (2_000, 2500.1, 2500.3))
        self.assertEqual(updated["tick_age_ms"], 4)
        self.assertEqual(updated["positions"], snapshot["positions"])
        self.assertIs(updated["market"], snapshot["market"])
        self.assertEqual(snapshot["bid"], 2500.0)

    def test_bad_fast_tick_is_rejected(self):
        snapshot = {"connected": True, "bid": 1.0, "ask": 1.1}
        source = TickSource(NS(bid=2.0, ask=1.0, time_msc=2_000))
        updated, changed, signature = gateway.refresh_fast_tick(snapshot, source, "XAUUSD", now_ms=2_001)
        self.assertIs(updated, snapshot)
        self.assertFalse(changed)
        self.assertIsNone(signature)

    def test_only_tick_entries_and_owned_closes_use_fast_dispatch(self):
        tick_open = {"kind": "open", "signal": {"key": "tick:123"}}
        bar_open = {"kind": "open", "signal": {"key": "bar:123"}}
        close = {"kind": "close", "signal": None}
        self.assertTrue(execution.is_fast_path(tick_open))
        self.assertTrue(execution.is_fast_path(close))
        self.assertFalse(execution.is_fast_path(bar_open))
        self.assertEqual(execution.FAST_QUEUE_TARGET_MS, 20)

    def test_durable_journal_and_regular_order_check_remain_present(self):
        source = (ROOT / "gateway" / "execution.py").read_text(encoding="utf-8")
        self.assertIn("PRAGMA synchronous=FULL", source)
        self.assertIn("self.db.commit()", source)
        self.assertIn("mt5.order_send(request)", source)
        self.assertIn("mt5.order_check(request)", source)
        self.assertIn("order_check_skipped", source)
        self.assertLess(source.index("self.db.commit()", source.index("INSERT INTO sends")), source.index("mt5.order_send(request)"))


if __name__ == "__main__":
    unittest.main()
