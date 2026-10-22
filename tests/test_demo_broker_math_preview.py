"""Fake broker tests prove cost-aware Demo-only preview cannot send orders."""
import importlib.util
from pathlib import Path
from types import SimpleNamespace as NS
import sys
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/"gateway"))
spec=importlib.util.spec_from_file_location(
    "demo_broker_math_preview",ROOT/"gateway"/"demo_broker_math_preview.py")
preview=importlib.util.module_from_spec(spec)
spec.loader.exec_module(preview)

class FakeMT5:
    ACCOUNT_TRADE_MODE_DEMO=0
    ORDER_TYPE_BUY=0
    ORDER_TYPE_SELL=1
    def __init__(self):
        self.account=NS(login=123,server="Broker-Demo",currency="USD",
            trade_mode=0,equity=500.,margin_free=250.)
        self.info=NS(visible=True,volume_min=0.01,volume_max=10.,
            volume_step=0.01,trade_tick_size=0.1)
        self.tick=NS(bid=5000.,ask=5000.2,time_msc=1000)
        self.calculations=0
        self.send_calls=0
        self.check_calls=0
        self.switch=False
        self.broken_profit=False
    def terminal_info(self):return NS(connected=True)
    def account_info(self):
        if self.switch and self.calculations:
            return NS(login=999,server="Other",currency="USD",
                trade_mode=0,equity=500.,margin_free=250.)
        return self.account
    def symbol_info(self,_):return self.info
    def symbol_info_tick(self,_):return self.tick
    def order_calc_profit(self,side,symbol,lots,entry,exit_price):
        self.calculations+=1
        if self.broken_profit:return None
        if side==self.ORDER_TYPE_BUY:return (exit_price-entry)*100*lots
        return (entry-exit_price)*100*lots
    def order_calc_margin(self,side,symbol,lots,price):
        self.calculations+=1
        return 250*lots
    def order_send(self,_):
        self.send_calls+=1
        raise AssertionError("Preview must not send orders")
    def order_check(self,_):
        self.check_calls+=1
        raise AssertionError("Preview must not authorize orders")

class Tests(unittest.TestCase):
    def calc(self,broker,**kwargs):
        return preview.calculate(broker,"XAUUSD","buy",0.01,4999.0,
            take_profit=5002.,now_ms=2000,**kwargs)
    def test_buy_real_contract_math_costs_and_no_trade_calls(self):
        b=FakeMT5()
        v=self.calc(b)
        self.assertEqual(v["source"],
            "MT5 Demo order_calc_profit and order_calc_margin, NOT order_check")
        self.assertAlmostEqual(v["estimated_roundtrip_commission"],0.07)
        self.assertAlmostEqual(v["estimated_loss_if_stop_including_assumed_costs"],1.47)
        self.assertAlmostEqual(v["estimated_initial_margin"],2.5)
        self.assertEqual(v["broker_order_sent"],False)
        self.assertEqual(v["actual_broker_approval"],False)
        self.assertEqual(b.send_calls,0)
        self.assertEqual(b.check_calls,0)
    def test_commission_scales_with_lots_in_account_currency(self):
        for lots, fee in ((0.01,0.07),(0.1,0.7),(1.0,7.0)):
            with self.subTest(lots=lots):
                b=FakeMT5()
                v=preview.calculate(b,"XAUUSD","buy",lots,4999.0,now_ms=2000)
                self.assertAlmostEqual(v["estimated_roundtrip_commission"],fee)
    def test_microaccount_does_not_pass_broker_or_risk_admission(self):
        b=FakeMT5()
        b.account.equity=50.
        b.account.margin_free=1.
        v=self.calc(b)
        self.assertEqual(v["scenario_stop_risk_exceeds_budget"],True)
        self.assertEqual(v["scenario_margin_plus_fees_exceeds_free_margin"],True)
        self.assertEqual(v["actual_broker_approval"],False)
    def test_sell_uses_bid_for_entry_and_loss_side_for_stop(self):
        b=FakeMT5()
        v=preview.calculate(b,"XAUUSD","sell",0.01,5001.0,
            take_profit=4998.0,now_ms=2000)
        self.assertAlmostEqual(v["assumed_entry_with_slippage"],4999.9)
        self.assertAlmostEqual(v["assumed_stop_fill_with_slippage"],5001.1)
        self.assertGreater(v["estimated_loss_if_stop_including_assumed_costs"],0)
        self.assertGreater(v["estimated_broker_target_gross"],0)
    def test_refuses_real_account_stale_quote_and_broker_min_lot(self):
        b=FakeMT5()
        b.account.trade_mode=1
        with self.assertRaises(preview.PreviewError):self.calc(b)
        self.assertEqual(b.calculations,0)
        b=FakeMT5()
        with self.assertRaises(preview.PreviewError):
            preview.calculate(b,"XAUUSD","buy",0.01,4999.,now_ms=9000)
        b=FakeMT5()
        with self.assertRaises(preview.PreviewError):
            preview.calculate(b,"XAUUSD","buy",0.005,4999.,now_ms=2000)
    def test_refuses_invalid_stops_and_targets_without_broker_calls(self):
        b=FakeMT5()
        with self.assertRaises(preview.PreviewError):
            preview.calculate(b,"XAUUSD","buy",0.01,5000.0,now_ms=2000)
        with self.assertRaises(preview.PreviewError):
            preview.calculate(b,"XAUUSD","buy",0.01,4999.0,
                take_profit=5000.1,now_ms=2000)
        self.assertEqual(b.calculations,0)
    def test_broker_loss_estimate_missing_or_account_switched_aborts(self):
        b=FakeMT5();b.broken_profit=True
        with self.assertRaises(preview.PreviewError):self.calc(b)
        b=FakeMT5();b.switch=True
        with self.assertRaisesRegex(preview.PreviewError,"changed"):
            self.calc(b)
    def test_no_silent_volume_rounding(self):
        b=FakeMT5()
        with self.assertRaisesRegex(preview.PreviewError,"min/max/step"):
            preview.calculate(b,"XAUUSD","buy",0.015,4999.0,now_ms=2000)

if __name__=="__main__":
    unittest.main()
