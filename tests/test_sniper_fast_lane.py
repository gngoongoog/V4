import importlib.util
from pathlib import Path
from types import SimpleNamespace as NS
import sys
import tempfile
import unittest

ROOT = Path(__file__).parents[1]
sys.path.insert(0, str(ROOT / "gateway"))


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

    def test_pulse_payload_excludes_heavy_reconciled_state(self):
        snapshot = {
            "login": 17,
            "server": "Demo",
            "currency": "USD",
            "symbol": "XAUUSD",
            "bid": 2500.1,
            "ask": 2500.3,
            "quote_time_ms": 2001,
            "tick_age_ms": 2,
            "positions": [{"ticket": 1}],
            "market": {"m1": [1]},
            "dom": [{"price": 1}],
        }
        payload = gateway.pulse_payload(snapshot)
        self.assertEqual(set(payload), {
            "login", "server", "currency", "symbol", "bid", "ask", "quote_time_ms", "tick_age_ms"
        })
        self.assertNotIn("positions", payload)
        self.assertNotIn("market", payload)
        self.assertNotIn("dom", payload)

    def test_quote_origin_native_poll_and_mailbox_overflow_are_explicit_and_bounded(self):
        snapshot = {
            "login": 17, "server": "Demo", "currency": "USD",
            "symbol": "XAUUSD", "bid": 2500.1, "ask": 2500.3,
            "source_quote_time_ms": 2000, "quote_time_ms": 2001,
            "tick_age_ms": 1, "positions": [{"ticket": 9}]
        }
        for origin in ("native", "poll"):
            p = gateway.pulse_payload(snapshot, quote_source=origin,
                native_dropped_total=12)
            self.assertEqual(p["quote_source"], origin)
            self.assertEqual(p["native_dropped_total"], 12)
            self.assertEqual(p["source_quote_time_ms"], 2000)
            self.assertEqual(p["quote_time_ms"], 2001)
            self.assertNotIn("positions", p)
        legacy = gateway.pulse_payload(snapshot)
        self.assertNotIn("quote_source", legacy)
        self.assertNotIn("native_dropped_total", legacy)
        for invalid_source in ("", "paper", "fake-native", "trade"):
            with self.assertRaises(ValueError):
                gateway.pulse_payload(snapshot, quote_source=invalid_source)
        for invalid_counter in (-1, 3.5, True, "14"):
            with self.assertRaises(ValueError):
                gateway.pulse_payload(snapshot, quote_source="native",
                    native_dropped_total=invalid_counter)

    def test_only_tick_entries_and_owned_closes_use_fast_dispatch(self):
        tick_open = {"kind": "open", "signal": {"key": "tick:123"}}
        bar_open = {"kind": "open", "signal": {"key": "bar:123"}}
        close = {"kind": "close", "signal": None}
        self.assertTrue(execution.is_fast_path(tick_open))
        self.assertTrue(execution.is_fast_path(close))
        self.assertFalse(execution.is_fast_path(bar_open))
        self.assertEqual(execution.FAST_QUEUE_TARGET_MS, 20)

    def test_prefetched_pulse_work_skips_second_gateway_fetch_and_history(self):
        with tempfile.TemporaryDirectory() as directory:
            executor = execution.BrokerExecutor(Path(directory) / "journal.sqlite")
            calls = []
            try:
                def api(path, payload=None):
                    calls.append((path, payload))
                    raise AssertionError("prefetched no-command pulse must not call Core again")

                delay = executor.cycle(None, api, work={
                    "command": None,
                    "history_from_ms": 123,
                    "account": {"login": 17, "server": "Demo", "currency": "USD"},
                    "poll_ms": 100,
                    "pulse_fast_path": True,
                    "pulse_to_work_us": 350,
                })
                self.assertEqual(calls, [])
                self.assertAlmostEqual(delay, 0.1)
            finally:
                executor.close()

    def test_pulse_uses_put_and_execution_only_core_endpoint(self):
        gateway_source = (ROOT / "gateway" / "mt5_gateway.py").read_text(encoding="utf-8")
        core_source = (ROOT / "crates" / "runtime" / "src" / "execution.rs").read_text(encoding="utf-8")
        self.assertIn("/v1/execution/pulse", gateway_source)
        self.assertIn("method='PUT'", gateway_source)
        self.assertIn('method == "PUT" && path == "/v1/execution/pulse"', core_source)
        self.assertIn('work["pulse_fast_path"]', core_source)
        self.assertIn('work["pulse_to_work_us"]', core_source)

    def test_durable_journal_and_regular_order_check_remain_present(self):
        source = (ROOT / "gateway" / "execution.py").read_text(encoding="utf-8")
        self.assertIn("PRAGMA synchronous=FULL", source)
        self.assertIn("self.db.commit()", source)
        self.assertIn("mt5.order_send(request)", source)
        self.assertIn("mt5.order_check(request)", source)
        self.assertIn("order_check_skipped", source)
        self.assertIn("authorize(cmd['id'])", source)
        self.assertLess(source.index("self.db.commit()", source.index("INSERT INTO sends")), source.index("mt5.order_send(request)"))


if __name__ == "__main__":
    unittest.main()
