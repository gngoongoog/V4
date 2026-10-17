from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).parents[1]
spec = importlib.util.spec_from_file_location("check_runtime_acceptance", ROOT / "gateway/check_runtime.py")
acceptance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(acceptance)


class DemoAcceptanceTests(unittest.TestCase):
    def identity(self):
        return {
            "server": "Broker-Demo",
            "login": 123456,
            "symbol": "XAUUSD",
            "currency": "USD",
            "account_fingerprint": acceptance._fingerprint("Broker-Demo", 123456, "XAUUSD", "USD"),
        }

    def item(self, intent: str, created: int, *, fast=True, source="native", queue=10.0,
             send=5.0, state="filled", skipped=True):
        signal_key = f"tick:{created}" if fast else f"bar:{created}"
        request = {
            "id": intent,
            "created_ms": created,
            "brain": "system",
            "kind": "open",
            "login": 123456,
            "server": "Broker-Demo",
            "currency": "USD",
            "symbol": "XAUUSD",
            "target": "demo",
            "signal": {"key": signal_key},
        }
        result = {
            "id": intent,
            "state": state,
            "fast_path": fast,
            "order_check_skipped": skipped,
            "prefetched_work": fast,
            "core_pulse_us": 2500 if fast else None,
            "queue_to_gateway_ms": 2.0,
            "authorize_ms": 1.0,
            "journal_ms": 1.5,
            "post_journal_authorize_ms": 0.75,
            "post_journal_account_ms": 0.25,
            "check_ms": 0.0 if fast else 2.0,
            "pre_send_ms": 6.0 if fast else 8.0,
            "queue_to_send_ms": queue,
            "send_ms": send,
            "total_ms": queue + send,
            "queue_to_send_target_met": queue <= 20.0,
            "retcode": 10009,
            "pricing": {"spread_points": 10.0},
        }
        return {"request": request, "state": "observed", "result": result, "source": source}

    def test_percentile_is_deterministic(self):
        self.assertEqual(acceptance.percentile([], 0.95), None)
        self.assertEqual(acceptance.percentile([7], 0.95), 7)
        self.assertAlmostEqual(acceptance.percentile([0, 10, 20, 30], 0.50), 15.0)
        self.assertAlmostEqual(acceptance.percentile([0, 10, 20, 30], 0.95), 28.5)

    def test_demo_context_rejects_live_and_mismatch(self):
        execution = {
            "settings": {"target": "demo", "login": 123456, "server": "Broker-Demo", "currency": "USD"},
            "account_matches": True,
            "context": json.dumps(["Broker-Demo", 123456, "XAUUSD", "USD"]),
        }
        identity = acceptance._context(execution)
        self.assertEqual(identity["symbol"], "XAUUSD")
        self.assertNotIn("123456", identity["account_fingerprint"])
        live = dict(execution, settings=dict(execution["settings"], target="live"))
        with self.assertRaisesRegex(RuntimeError, "refuses a Live"):
            acceptance._context(live)
        mismatch = dict(execution, account_matches=False)
        with self.assertRaisesRegex(RuntimeError, "currently connected Demo"):
            acceptance._context(mismatch)

    def test_source_label_only_applies_to_new_commands_and_is_preserved(self):
        identity = self.identity()
        samples = {}
        old = self.item("old", 1000)
        new = self.item("new", 3000)
        execution = {"commands": [old, new]}
        acceptance.merge_execution(samples, execution, identity, "native", session_start_ms=2000)
        self.assertNotIn("old", samples)
        self.assertEqual(samples["new"]["source"], "native")
        # A later poll capture may update command state/result, but must not relabel
        # the original controlled native session.
        new["result"]["state"] = "partial"
        acceptance.merge_execution(samples, {"commands": [new]}, identity, "poll", session_start_ms=4000)
        self.assertEqual(samples["new"]["source"], "native")
        self.assertEqual(samples["new"]["result_state"], "partial")

    def test_report_splits_native_and_poll_and_measures_target(self):
        identity = self.identity()
        samples = {}
        for index, queue in enumerate((8.0, 10.0, 12.0)):
            item = self.item(f"n{index}", 2000 + index, queue=queue)
            acceptance.merge_execution(samples, {"commands": [item]}, identity, "native", 1000)
        for index, queue in enumerate((9.0, 11.0)):
            item = self.item(f"p{index}", 3000 + index, queue=queue)
            acceptance.merge_execution(samples, {"commands": [item]}, identity, "poll", 2500)
        metadata = {"account_fingerprint": identity["account_fingerprint"], "target": "demo"}
        report, rows = acceptance.build_report(metadata, samples, minimum_fast_samples=5)
        self.assertEqual(report["result"], "TARGET_MET")
        self.assertTrue(report["characterization_complete"])
        self.assertEqual(report["fast_path"]["sources"]["native"]["count"], 3)
        self.assertEqual(report["fast_path"]["sources"]["poll"]["count"], 2)
        self.assertTrue(report["contracts"]["fast_open_skips_order_check"])
        self.assertTrue(report["contracts"]["durable_journal_timing_present_before_recorded_send"])
        self.assertLessEqual(report["fast_path"]["timings"]["queue_to_send_ms"]["p99"], 20.0)
        self.assertEqual(report["fast_path"]["timings"]["post_journal_authorize_ms"]["count"], 5)
        self.assertEqual(report["fast_path"]["timings"]["post_journal_account_ms"]["count"], 5)
        self.assertAlmostEqual(rows[0]["other_pre_send_ms"], 2.5)
        serialized = json.dumps(report)
        self.assertNotIn("123456", serialized)
        self.assertEqual(len(rows), 5)

    def test_legacy_samples_without_new_final_gate_timings_remain_readable(self):
        identity = self.identity()
        item = self.item("legacy", 4000)
        item["result"].pop("post_journal_authorize_ms")
        item["result"].pop("post_journal_account_ms")
        samples = {}
        acceptance.merge_execution(samples, {"commands": [item]}, identity, "native", 3000)
        row = samples["legacy"]
        self.assertIsNone(row["post_journal_authorize_ms"])
        self.assertIsNone(row["post_journal_account_ms"])
        self.assertAlmostEqual(row["other_pre_send_ms"], 3.5)

    def test_slow_characterization_and_regular_order_check_contract(self):
        identity = self.identity()
        samples = {}
        fast = self.item("slow", 5000, queue=35.0)
        acceptance.merge_execution(samples, {"commands": [fast]}, identity, "native", 4000)
        regular = self.item("regular", 6000, fast=False, queue=15.0, skipped=False)
        acceptance.merge_execution(samples, {"commands": [regular]}, identity, "auto", 0)
        report, _ = acceptance.build_report({"target": "demo"}, samples, minimum_fast_samples=1)
        self.assertEqual(report["result"], "CHARACTERIZED_SLOW")
        self.assertFalse(report["fast_path"]["p95_target_met"])
        self.assertTrue(report["contracts"]["regular_open_keeps_order_check"])

    def test_collector_source_is_read_only(self):
        source = (ROOT / "gateway/check_runtime.py").read_text(encoding="utf-8")
        self.assertIn("class CoreApi", source)
        self.assertIn("def get(self, path: str)", source)
        self.assertNotIn("/v1/start\"", source)
        self.assertNotIn("/v1/stop\"", source)
        self.assertNotIn("/v1/execution/result\"", source)
        self.assertNotIn("/v1/execution/settings\"", source)


if __name__ == "__main__":
    unittest.main()