"""Deterministic no-broker HVN source-quality tests.

These fixtures are synthetic ONLY for unit testing the calculator. They do
not claim historical XAUUSD trading success, real exchange size, or fill PnL.
"""
import math
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "gateway"))
from hvn_research import calculate_hvn

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

    def test_no_executable_broker_in_hvn_research_module(self):
        source=(Path(__file__).resolve().parents[1]/"gateway"/"hvn_research.py").read_text()
        self.assertNotIn("order_send(", source)
        self.assertNotIn("mt5.initialize(", source)


if __name__ == "__main__":
    unittest.main()
