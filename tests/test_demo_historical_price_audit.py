"""Pure fake-MT5 tests: exporter never writes to broker or infers quote fills."""
import importlib.util
from pathlib import Path
from types import SimpleNamespace as NS
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/"gateway"))
spec = importlib.util.spec_from_file_location(
    "demo_historical_price_audit", ROOT/"gateway"/"demo_historical_price_audit.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

START = 1_700_000_000_000
END = START + 60_000


def deal(ticket=11, side=0, price=5000.3, entry=0, symbol="XAUUSD"):
    return NS(ticket=ticket, position_id=91, type=side, entry=entry,
              symbol=symbol, volume=.04, price=price, time_msc=START+1000,
              profit=1.2, commission=-.14, swap=0., fee=0.)


class MockMT5:
    ACCOUNT_TRADE_MODE_DEMO = 0
    DEAL_TYPE_BUY = 0
    DEAL_TYPE_SELL = 1
    COPY_TICKS_ALL = -1

    def __init__(self):
        self.account = NS(login=777, server="Test-Demo", currency="USD", trade_mode=0)
        self.deals = [deal()]
        self.ticks = [{"time_msc":START+900, "bid":5000.0,"ask":5000.2}]
        self.deal_calls=0
        self.tick_calls=0
        self.send_calls=0
        self.switch=False

    def terminal_info(self):
        return NS(connected=True)

    def account_info(self):
        if self.switch and self.deal_calls:
            return NS(login=999, server="Other-Demo", currency="USD", trade_mode=0)
        return self.account

    def symbol_info(self,symbol):
        return NS(trade_tick_size=0.1)

    def history_deals_get(self,from_utc,to_utc):
        self.deal_calls+=1
        return self.deals

    def copy_ticks_range(self,symbol,from_utc,to_utc,flags):
        self.tick_calls+=1
        return self.ticks

    def order_send(self,request):
        self.send_calls+=1
        raise AssertionError("Read-only audit must never send an order")


class Tests(unittest.TestCase):
    def inspect(self,broker,**kwargs):
        return module.capture(broker,"XAUUSD",START,END,now_ms=END+1000,**kwargs)

    def test_buy_execution_deviation_vs_recent_prior_ask(self):
        mt5=MockMT5()
        v=self.inspect(mt5)
        self.assertEqual(v["rows"][0]["broker_deal_side"],"buy")
        self.assertEqual(v["deals_with_valid_prior_quote"],1)
        self.assertAlmostEqual(v["rows"][0]["indicative_adverse_deviation_ticks"],1.0)
        self.assertAlmostEqual(v["rows"][0]["historical_spread_ticks"],2.0)
        self.assertEqual(v["rows"][0]["reference_quote_age_ms"],100)
        self.assertAlmostEqual(v["rows"][0]["fees_and_commission_booked_on_supplied_deal"],-.14)
        self.assertFalse(v["quote_executability_verified"])
        self.assertFalse(v["complete_history"])
        self.assertEqual(mt5.send_calls,0)

    def test_sell_price_deviation_uses_bid_and_sign_is_adverse_positive(self):
        mt5=MockMT5()
        mt5.deals=[deal(side=1,price=4999.8)]
        v=self.inspect(mt5)
        self.assertAlmostEqual(v["rows"][0]["indicative_adverse_deviation_ticks"],2.)
        mt5.deals=[deal(side=1,price=5000.1)]
        v=self.inspect(mt5)
        self.assertAlmostEqual(v["rows"][0]["indicative_adverse_deviation_ticks"],-1.)

    def test_absent_or_future_only_ticks_are_not_backfilled(self):
        mt5=MockMT5()
        mt5.ticks=[{"time_msc":START+1100,"bid":5000.0,"ask":5000.2}]
        v=self.inspect(mt5)
        self.assertEqual(v["deals_missing_reference"],1)
        self.assertIsNone(v["rows"][0]["reference_quote_price"])
        self.assertIsNone(v["mean_indicative_adverse_deviation_ticks"])
        mt5.ticks=[]
        self.assertEqual(self.inspect(mt5)["deals_missing_reference"],1)
        mt5.ticks=[{"time_msc":START-2000,"bid":5000.0,"ask":5000.2}]
        self.assertEqual(self.inspect(mt5)["deals_missing_reference"],1)

    def test_real_account_is_refused_before_reading_history(self):
        mt5=MockMT5()
        mt5.account.trade_mode=1
        with self.assertRaises(module.DemoEvidenceError):
            self.inspect(mt5)
        self.assertEqual(mt5.deal_calls,0)
        self.assertEqual(mt5.tick_calls,0)

    def test_demo_account_switch_wipes_entire_audit(self):
        mt5=MockMT5()
        mt5.switch=True
        with self.assertRaisesRegex(module.DemoEvidenceError,"switched"):
            self.inspect(mt5)

    def test_duplicate_deal_tickets_or_missing_history_fail_closed(self):
        mt5=MockMT5()
        mt5.deals=[deal(ticket=11),deal(ticket=11)]
        with self.assertRaisesRegex(module.DemoEvidenceError,"Duplicate"):
            self.inspect(mt5)
        mt5.deals=None
        with self.assertRaisesRegex(module.DemoEvidenceError,"unavailable"):
            self.inspect(mt5)

    def test_inout_or_unsupported_side_is_not_inferred_from_quote(self):
        mt5=MockMT5()
        mt5.deals=[deal(entry=2)]
        v=self.inspect(mt5)
        self.assertIsNone(v["rows"][0]["indicative_adverse_deviation_ticks"])
        self.assertEqual(mt5.tick_calls,0)
        mt5.deals=[deal(side=7)]
        v=self.inspect(mt5)
        self.assertIsNone(v["rows"][0]["reference_quote_price"])

    def test_tick_history_unavailable_and_invalid_economics_rejected(self):
        mt5=MockMT5()
        mt5.ticks=None
        with self.assertRaisesRegex(module.DemoEvidenceError,"unavailable"):
            self.inspect(mt5)
        mt5.ticks=[]
        mt5.deals=[deal()]
        mt5.deals[0].fee=float("nan")
        with self.assertRaisesRegex(module.DemoEvidenceError,"Invalid deal"):
            self.inspect(mt5)

    def test_requires_utc_bounded_window_and_valid_quote_staleness(self):
        mt5=MockMT5()
        for age in (0,3001):
            with self.assertRaises(module.DemoEvidenceError):
                self.inspect(mt5,max_quote_age_ms=age)
        with self.assertRaises(module.DemoEvidenceError):
            module.capture(mt5,"XAUUSD",END,START,now_ms=END+1000)
        with self.assertRaises(module.DemoEvidenceError):
            module.capture(mt5,"XAUUSD",START,END+24*60*60*1000,now_ms=END+1000)


if __name__ == "__main__":
    unittest.main()
