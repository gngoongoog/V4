"""Deterministic no-broker HVN source-quality tests.

These fixtures are synthetic ONLY for unit testing the calculator. They do
not claim historical XAUUSD trading success, real exchange size, or fill PnL.
"""
import math
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "gateway"))
from hvn_research import calculate_hvn, collect_mt5_hvn

NOW = 1_800_000_000_000
KW = dict(server="Broker-Demo", login=42, symbol="XAUUSDm", currency="USD",
          tick_size=0.01, now_ms=NOW)


def quotes(count=150, *, source_end=NOW-1000):
    result = []
    # 10 separate observed price levels, one sharply dominant local peak.
    for i in range(count):
        level = 5 if i % 3 != 0 else i % 10
        bid = 2000 + level * 0.01
        result.append({"time_msc": source_end - (count-1-i)*1000,
                       "bid": bid, "ask": bid + 0.02,
                       "last": 0, "volume_real": 0, "flags": 6})
    return result


class HvnResearch(unittest.TestCase):
    def test_quote_counts_are_labeled_as_quote_activity_not_market_volume(self):
        result = calculate_hvn(quotes(), **KW)
        self.assertTrue(result["available"], result)
        self.assertEqual(result["source"], "mt5_quote_tick_activity")
        self.assertEqual(result["unit"], "observed_quote_events")
        self.assertFalse(result["exchange_wide"])
        self.assertFalse(result["trade_authorized"])
        self.assertTrue(result["read_only"])
        self.assertGreater(result["nodes"][0]["share"], 0.20)
        self.assertIn("NOT traded volume", result["note"])

    def test_genuine_broker_real_volume_is_never_pooled_with_quote_counts(self):
        rows = quotes(150, source_end=NOW-151000)
        for i in range(90):
            price = 2000 + (5 if i%3 else i%10)*0.01
            rows.append({"time_msc": NOW-90000+i*1000, "last": price,
                         "volume_real": 2.5, "bid": price-0.01, "ask": price+0.01,
                         "flags": 24})
        result = calculate_hvn(rows, **KW)
        self.assertTrue(result["available"], result)
        self.assertEqual(result["source"], "mt5_broker_trade_volume_real")
        self.assertEqual(result["observations"], 90)
        self.assertAlmostEqual(result["total_activity"], 225.0)
        self.assertFalse(result["exchange_wide"])

    def test_mixed_trade_and_quote_flags_preserve_quote_activity_fallback(self):
        # A quote change and a trade update can share one MT5 tick row.
        # With fewer than the required number of observed trade updates,
        # the quote fallback still counts *all* actual bid/ask changes.
        rows = quotes(150)
        for row in rows[:60]:
            row["flags"] = 6 | 8 | 16
            row["last"] = 2000.05
            row["volume_real"] = 3.5
        result = calculate_hvn(rows, **KW)
        self.assertTrue(result["available"], result)
        self.assertEqual(result["source"], "mt5_quote_tick_activity")
        self.assertEqual(result["observations"], 150)
        self.assertEqual(result["total_activity"], 150)
        self.assertEqual(result["trade_events_observed"], 60)
        self.assertEqual(result["quote_events_observed"], 150)
        self.assertEqual(result["unit"], "observed_quote_events")
        self.assertFalse(result["exchange_wide"])

    def test_mixed_flags_never_add_quote_counts_to_actual_trade_volume(self):
        rows = quotes(150)
        for i, row in enumerate(rows[:90]):
            row["flags"] = 6 | 8 | 16
            # Keep multiple observed Last levels; a single level would
            # correctly fail HVN's minimum distinct-price evidence gate.
            row["last"] = 2000 + (5 if i % 3 else i % 10) * 0.01
            row["volume_real"] = 2.5
        result = calculate_hvn(rows, **KW)
        self.assertTrue(result["available"], result)
        self.assertEqual(result["source"], "mt5_broker_trade_volume_real")
        self.assertEqual(result["observations"], 90)
        self.assertAlmostEqual(result["total_activity"], 225.0)
        self.assertEqual(result["trade_events_observed"], 90)
        self.assertEqual(result["quote_events_observed"], 150)
        self.assertNotEqual(result["total_activity"], 225.0 + 150)
        self.assertFalse(result["exchange_wide"])

    def test_quote_only_flags_cannot_recount_old_last_trade_volume(self):
        rows = quotes()
        for row in rows:
            row["last"] = 2000.20
            row["volume_real"] = 500
            row["flags"] = 6  # Only bid/ask changed; old Last is carried forward.
        result = calculate_hvn(rows, **KW)
        self.assertTrue(result["available"], result)
        self.assertEqual(result["source"], "mt5_quote_tick_activity")
        self.assertEqual(result["observations"], len(rows))
        self.assertEqual(result["total_activity"], len(rows))

    def test_missing_or_malformed_flags_never_create_reported_trade_volume(self):
        # Broker/adapter rows without reliable MT5 flags must not turn a
        # carried-forward Last/volume_real into transactions or quote events.
        for invalid_flags in (None, "24", 24.0, 24.8, True, -1, 128):
            with self.subTest(flags=invalid_flags):
                rows = quotes()
                for row in rows:
                    row["last"] = 2000.05
                    row["volume_real"] = 500.0
                    row["flags"] = invalid_flags
                result = calculate_hvn(rows, **KW)
                self.assertFalse(result["available"], result)
                self.assertEqual(result["nodes"], [])
                self.assertIn("flags", result["reason"])
        # A syntactically valid flag with no trade or quote bits must also
        # never count previously stored Last values as new broker executions.
        rows = quotes()
        for row in rows:
            row.update(last=2000.05, volume_real=500.0, flags=0)
        result = calculate_hvn(rows, **KW)
        self.assertFalse(result["available"], result)
        self.assertEqual(result["source"], "none")

    def test_illiquid_or_missing_or_bad_contract_has_no_nodes(self):
        for ticks in [[],quotes(22),None]:
            result=calculate_hvn(ticks, **KW)
            self.assertFalse(result["available"])
            self.assertEqual(result["nodes"], [])
        self.assertFalse(calculate_hvn(quotes(), **{**KW, "tick_size":0})["available"])
        self.assertFalse(calculate_hvn(quotes(), **{**KW, "server":""})["available"])

    def test_reversed_ticks_are_rejected_not_sorted_into_fake_history(self):
        rows = quotes()
        rows[33],rows[34] = rows[34],rows[33]
        out=calculate_hvn(rows, **KW)
        self.assertFalse(out["available"])
        self.assertIn("Out-of-order", out["reason"])

    def test_future_timestamps_do_not_change_causal_observations(self):
        baseline=calculate_hvn(quotes(), **KW)
        rows=quotes()
        rows.append({"time_msc":NOW+50_000,"bid":2000.05,
                     "ask":2000.07,"last":0,"volume_real":0,"flags":6})
        after=calculate_hvn(rows, **KW)
        self.assertEqual(after["nodes"], baseline["nodes"])
        self.assertEqual(after["observations"], baseline["observations"])

    def test_nonfinite_and_broken_quote_samples_do_not_become_volume(self):
        rows=quotes()
        for x in rows[10:50]:
            x["bid"]=math.nan
        result=calculate_hvn(rows, **KW)
        self.assertTrue(result["available"], result)
        self.assertEqual(result["observations"], 110)

    def test_no_rows_from_one_broker_can_be_implicitly_labeled_as_market_wide(self):
        out=calculate_hvn(quotes(), **{**KW, "login":44})
        self.assertEqual(out["identity"]["login"],44)
        self.assertEqual(out["identity"]["symbol"],"XAUUSDm")
        self.assertTrue(out["broker_only"])
        self.assertFalse(out["exchange_wide"])

    def test_read_only_mt5_collect_checks_identity_before_and_after_history(self):
        from types import SimpleNamespace
        class Broker:
            COPY_TICKS_ALL = 0
            def __init__(self):
                self.calls = []
                self.server = KW["server"]
            def account_info(self):
                self.calls.append("account")
                return SimpleNamespace(login=42, server=self.server, currency="USD")
            def positions_get(self):
                self.calls.append("positions")
                return ()
            def orders_get(self):
                self.calls.append("orders")
                return ()
            def symbol_info(self, symbol):
                self.calls.append(("symbol", symbol))
                return SimpleNamespace(trade_tick_size=0.01)
            def copy_ticks_range(self, symbol, start, end, flags):
                self.calls.append(("ticks",symbol,flags,start.utcoffset(),end.utcoffset()))
                return quotes()
        broker = Broker()
        out = collect_mt5_hvn(broker, **{k:KW[k] for k in
            ("server","login","symbol","currency","now_ms")})
        self.assertTrue(out["available"], out)
        self.assertEqual(broker.calls.count("account"),2)
        self.assertEqual(out["identity"]["symbol"],"XAUUSDm")
        tick_event=next(c for c in broker.calls if isinstance(c,tuple) and c[0]=="ticks")
        self.assertEqual(tick_event[1:3],("XAUUSDm",0))
        self.assertEqual(tick_event[3].total_seconds(),0)
        self.assertEqual(tick_event[4].total_seconds(),0)
        self.assertEqual(broker.calls.count("positions"),2)
        self.assertEqual(broker.calls.count("orders"),2)

    def test_changing_mt5_account_while_reading_withdraws_all_hvn_levels(self):
        from types import SimpleNamespace
        class Switched:
            COPY_TICKS_ALL = 0
            calls = 0
            def account_info(self):
                self.calls += 1
                return SimpleNamespace(login=42 if self.calls==1 else 43,
                    server="Broker-Demo",currency="USD")
            def positions_get(self): return ()
            def orders_get(self): return ()
            def symbol_info(self,symbol):
                return SimpleNamespace(trade_tick_size=0.01)
            def copy_ticks_range(self,*args):
                return quotes()
        out=collect_mt5_hvn(Switched(), **{k:KW[k] for k in
            ("server","login","symbol","currency","now_ms")})
        self.assertFalse(out["available"])
        self.assertEqual(out["nodes"],[])
        self.assertIn("changed",out["reason"])

    def test_manual_position_or_unknown_order_book_blocks_tick_history(self):
        from types import SimpleNamespace
        class Broker:
            COPY_TICKS_ALL=0
            def __init__(self,positions=(),orders=()):
                self.positions=positions
                self.orders=orders
                self.reads=0
            def account_info(self):
                return SimpleNamespace(login=42,server="Broker-Demo",currency="USD")
            def positions_get(self): return self.positions
            def orders_get(self): return self.orders
            def symbol_info(self,symbol):
                return SimpleNamespace(trade_tick_size=.01)
            def copy_ticks_range(self,*args):
                self.reads+=1
                return quotes()
        for positions,orders in [([object()],()),((),[object()]),(None,()),((),None)]:
            mt5=Broker(positions,orders)
            result=collect_mt5_hvn(mt5,**{k:KW[k] for k in
                ("server","login","symbol","currency","now_ms")})
            self.assertFalse(result["available"])
            self.assertEqual(result["nodes"],[])
            self.assertEqual(mt5.reads,0,"Never start a blocking tick read for unknown or nonflat broker")

    def test_manual_position_opened_during_history_invalidates_result(self):
        from types import SimpleNamespace
        class Broker:
            COPY_TICKS_ALL=0
            def __init__(self): self.has_position=False
            def account_info(self):
                return SimpleNamespace(login=42,server="Broker-Demo",currency="USD")
            def positions_get(self): return [object()] if self.has_position else ()
            def orders_get(self): return ()
            def symbol_info(self,symbol):
                return SimpleNamespace(trade_tick_size=.01)
            def copy_ticks_range(self,*args):
                self.has_position=True
                return quotes()
        result=collect_mt5_hvn(Broker(),**{k:KW[k] for k in
            ("server","login","symbol","currency","now_ms")})
        self.assertFalse(result["available"])
        self.assertEqual(result["nodes"],[])
        self.assertIn("nonflat",result["reason"])

    def test_gateway_reports_unavailable_on_unexpected_collector_exception(self):
        from unittest.mock import patch
        from mt5_gateway import complete_hvn_job
        job = dict(id="test-job", server=KW["server"], login=42,
                   symbol=KW["symbol"], currency="USD", now_ms=NOW,
                   window_ms=600_000)
        receipts = []
        def api(path, payload):
            receipts.append((path, payload))
        with patch("hvn_research.collect_mt5_hvn",
                   side_effect=RuntimeError("Unexpected internal MT5 failure")):
            complete_hvn_job(None, api, job)
        self.assertEqual(len(receipts), 1)
        self.assertEqual(receipts[0][0], "/v1/gateway/hvn/result")
        self.assertEqual(receipts[0][1]["id"], job["id"])
        result = receipts[0][1]["report"]
        self.assertEqual(result["identity"]["login"], 42)
        self.assertEqual(result["window_ms"], job["window_ms"])
        self.assertEqual(result["source"], "none")
        self.assertFalse(result["available"])
        self.assertTrue(result["read_only"])
        self.assertFalse(result["trade_authorized"])
        self.assertEqual(result["nodes"], [])
        self.assertIn("RuntimeError", result["reason"])
        self.assertNotIn("Unexpected internal MT5 failure", result["reason"])

    def test_gateway_normal_hvn_completion_preserves_verified_report(self):
        from unittest.mock import patch
        from mt5_gateway import complete_hvn_job
        job = dict(id="normal-job", server=KW["server"], login=42,
                   symbol=KW["symbol"], currency="USD", now_ms=NOW,
                   window_ms=600_000)
        source_report = calculate_hvn(quotes(), **KW)
        receipts = []
        with patch("hvn_research.collect_mt5_hvn", return_value=source_report) as collector:
            complete_hvn_job(None, lambda path, payload: receipts.append((path, payload)), job)
        collector.assert_called_once()
        self.assertEqual(receipts[0][0], "/v1/gateway/hvn/result")
        self.assertIs(receipts[0][1]["report"], source_report)

    def test_hvn_completion_outbox_retries_transport_without_recollecting_ticks(self):
        from mt5_gateway import HvnResultOutbox
        outbox = HvnResultOutbox()
        receipt = {"id": "job-1", "report": {"available": False}}
        calls = []
        def fail_once(path, payload):
            calls.append((path, payload))
            if len(calls) == 1:
                raise OSError("Transient localhost connection loss")
        with self.assertRaises(OSError):
            outbox.submit(fail_once, receipt)
        self.assertIs(outbox.payload, receipt)
        self.assertTrue(outbox.flush(fail_once))
        self.assertIsNone(outbox.payload)
        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[0][1], calls[1][1],
                         "Retry exact completed receipt, not a re-read")
        self.assertFalse(outbox.flush(fail_once))

    def test_hvn_completion_outbox_retires_duplicate_or_rejected_response(self):
        import urllib.error
        from mt5_gateway import HvnResultOutbox
        outbox = HvnResultOutbox()
        receipt = {"id": "job-2", "report": {"available": False}}
        def rejected(path, payload):
            raise urllib.error.HTTPError(path, 400, "No outstanding HVN request", {}, None)
        self.assertFalse(outbox.submit(rejected, receipt))
        self.assertIsNone(outbox.payload)
        self.assertFalse(outbox.flush(rejected))
        # Authorization/transport failures must NOT silently discard a
        # completed read: the retry/exit fence remains fail-closed.
        def unauthorized(path, payload):
            raise urllib.error.HTTPError(path, 401, "Unauthorized", {}, None)
        with self.assertRaises(urllib.error.HTTPError):
            outbox.submit(unauthorized, receipt)
        self.assertIs(outbox.payload, receipt)

    def test_no_executable_broker_in_hvn_research_module(self):
        source=(Path(__file__).resolve().parents[1]/"gateway"/"hvn_research.py").read_text()
        self.assertNotIn("order_send(", source)
        self.assertNotIn("mt5.initialize(", source)


if __name__ == "__main__":
    unittest.main()
