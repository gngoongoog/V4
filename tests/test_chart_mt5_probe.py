"""Broker-free tests for the read-only seven-timeframe MT5 Demo probe."""
from datetime import datetime, timezone
import importlib.util
from pathlib import Path
from types import SimpleNamespace as N
import unittest

SPEC=importlib.util.spec_from_file_location(
    "chart_mt5_probe", Path(__file__).resolve().parents[1]/"scripts"/"check_mt5_chart_readonly.py"
)
PROBE=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROBE)
NOW=1_800_000_000_000
NOW_S=NOW//1000


class FakeDemo:
    ACCOUNT_TRADE_MODE_DEMO=0
    TIMEFRAME_M1=1
    TIMEFRAME_M5=5
    TIMEFRAME_M15=15
    TIMEFRAME_M30=30
    TIMEFRAME_H1=60
    TIMEFRAME_H4=240
    TIMEFRAME_D1=1440

    def __init__(self):
        self.account=N(login=17,server="Example-Demo",trade_mode=0)
        self.after=None
        self.account_calls=0
        self.quote_ms=NOW-500
        self.rates={}
        self.calls=[]
        for _,key,seconds in PROBE.FRAMES:
            tf=getattr(self,key)
            boundary=NOW_S//seconds*seconds
            self.rates[tf]=[dict(time=boundary-(180-i)*seconds,
                open=2477.5,high=2478.0,low=2477.0,close=2477.8,
                tick_volume=100+i) for i in range(180)]

    def account_info(self):
        self.account_calls+=1
        return self.after if self.after is not None and self.account_calls>1 else self.account

    def terminal_info(self):
        return N(connected=True)

    def symbol_info(self,symbol):
        return N(visible=True) if symbol=="XAUUSD" else None

    def symbol_info_tick(self,symbol):
        return N(time_msc=self.quote_ms,bid=2479.1,ask=2479.3)

    def copy_rates_from_pos(self,symbol,tf,start,count):
        self.calls.append((symbol,tf,start,count))
        return [dict(row) for row in self.rates[tf]]

    def order_send(self,*args,**kwargs):
        raise AssertionError("READ-ONLY acceptance MUST NOT call order_send")


class ReadOnlyChartProbeTests(unittest.TestCase):
    def test_7_frames_are_real_readonly_closed_bars(self):
        broker=FakeDemo()
        result=PROBE.inspect(broker,"XAUUSD",NOW)
        self.assertEqual(result["status"],"DEMO_FEED_RECENT")
        self.assertTrue(result["account_unchanged"])
        self.assertTrue(result["quote_recent"])
        self.assertEqual(set(result["frames"]),{label for label,_,_ in PROBE.FRAMES})
        self.assertTrue(all(value["history_integrity"]=="PASS" for value in result["frames"].values()))
        self.assertEqual(len(broker.calls),7)
        self.assertTrue(all(call[2:]==(1,180) for call in broker.calls))
        self.assertFalse(result["execution_performed"])
        self.assertNotIn("17",repr(result.get("account_login")))

    def test_real_account_is_refused_before_history_reads(self):
        broker=FakeDemo();broker.account.trade_mode=2
        result=PROBE.inspect(broker,"XAUUSD",NOW)
        self.assertEqual(result["status"],"DEMO_ACCOUNT_REQUIRED")
        self.assertEqual(broker.calls,[])

    def test_exact_symbol_no_auto_substitution(self):
        broker=FakeDemo()
        result=PROBE.inspect(broker,"XAUUSD-SUFFIX",NOW)
        self.assertEqual(result["status"],"EXACT_BROKER_SYMBOL_NOT_FOUND")
        self.assertEqual(broker.calls,[])

    def test_stale_quote_does_not_promote_history_to_live_feed(self):
        broker=FakeDemo();broker.quote_ms=NOW-25000
        result=PROBE.inspect(broker,"XAUUSD",NOW)
        self.assertEqual(result["status"],"DEMO_HISTORY_ONLY_OR_STALE")
        self.assertFalse(result["quote_recent"])

    def test_one_malformed_frame_does_not_corrupt_other_frames(self):
        broker=FakeDemo()
        broker.rates[1][50]["high"]=2477.4
        result=PROBE.inspect(broker,"XAUUSD",NOW)
        self.assertEqual(result["status"],"DEMO_HISTORY_INCOMPLETE")
        self.assertEqual(result["frames"]["M1"]["history_integrity"],"FAIL")
        self.assertEqual(result["frames"]["M5"]["history_integrity"],"PASS")

    def test_account_switch_after_snapshot_cannot_pass(self):
        broker=FakeDemo();broker.after=N(login=18,server="Example-Demo",trade_mode=0)
        result=PROBE.inspect(broker,"XAUUSD",NOW)
        self.assertEqual(result["status"],"ACCOUNT_SWITCHED_DURING_READ")
        self.assertFalse(result["account_unchanged"])

    def test_incomplete_or_forming_bars_not_repaired(self):
        broker=FakeDemo();broker.rates[5][-1]["time"]=NOW_S
        result=PROBE.inspect(broker,"XAUUSD",NOW)
        self.assertEqual(result["frames"]["M5"]["reason"],"TIMESTAMP_OR_OPEN_BAR")
        self.assertEqual(result["status"],"DEMO_HISTORY_INCOMPLETE")

    def test_broker_timezone_daily_bars_can_have_non_midnight_utc_open(self):
        broker=FakeDemo()
        # Shift the D1 session to the broker's UTC+2 midnight rather than UTC.
        for row in broker.rates[1440]:
            row["time"]-=7200
        result=PROBE.inspect(broker,"XAUUSD",NOW)
        self.assertEqual(result["frames"]["D1"]["history_integrity"],"PASS")


if __name__=="__main__":
    unittest.main()
