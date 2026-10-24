"""P0-D MT5 broker financial integrity. Pure mocks: no funded/DEMO sends."""
from __future__ import annotations
import copy
import math
from pathlib import Path
from types import SimpleNamespace as N
from unittest import TestCase, main
import sys
import tempfile

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'gateway'))
sys.path.insert(0,str(ROOT/'tests'))
from test_gateway import gateway,FakeMT5
from execution import BrokerExecutor
from test_broker_execution import Broker,command


class DailyNetLedgerIntegrity(TestCase):
    def test_actual_cost_components_booked_once_with_deposit_excluded(self):
        broker=FakeMT5()
        broker.deals=[N(type=0,profit=2.,commission=-.70,swap=-.25,fee=-.05),
                      N(type=2,profit=500,commission=0,swap=0,fee=0)]
        result=gateway.collect(broker,'XAUUSD')
        self.assertAlmostEqual(result['today_net'],1.)
        self.assertEqual(result['today_deals'],1)

    def test_missing_fee_is_unknown_not_no_commission(self):
        broker=FakeMT5()
        broker.deals=[N(type=0,profit=2.,commission=-.7,swap=-.25)]
        result=gateway.collect(broker,'XAUUSD')
        self.assertTrue(result['connected'])
        self.assertIsNone(result['today_net'])
        self.assertIsNone(result['today_deals'])
        self.assertIn('cost',result['error'].lower())

    def test_nonfinite_and_boolean_costs_cannot_reach_dashboard(self):
        for name,value in [('profit',float('nan')),('commission',float('inf')),
                           ('swap',None),('fee',True)]:
            with self.subTest(field=name):
                values=dict(type=0,profit=1.,commission=-.7,swap=-.25,fee=-.05)
                values[name]=value
                broker=FakeMT5()
                broker.deals=[N(**values)]
                result=gateway.collect(broker,'XAUUSD')
                self.assertIsNone(result['today_net'])
                self.assertIn('cost',result['error'].lower())

    def test_account_switch_with_same_login_server_but_different_trade_mode_is_rejected(self):
        broker=FakeMT5()
        original=broker.account
        changed=copy.copy(original)
        changed.trade_mode=2
        calls=0
        def account():
            nonlocal calls
            calls+=1
            return original if calls==1 else changed
        broker.account_info=account
        result=gateway.collect(broker,'XAUUSD')
        self.assertFalse(result['connected'])
        self.assertIsNone(result['today_net'])
        self.assertIn('Account changed',result['error'])


class DailyRiskGateMoneySource(TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.exe=BrokerExecutor(Path(self.tmp.name)/'journal.sqlite')
        self.broker=Broker()
        self.broker.account.trade_mode=2
        self.cmd=command()
        self.cmd.update(target='live',daily_breaker_enabled=True,daily_breaker_limit_percent=2.)

    def tearDown(self):
        self.exe.close()
        self.tmp.cleanup()

    def test_missing_commission_or_fee_does_not_pass_broker_daily_breaker(self):
        for missing in ('commission','fee','profit','swap'):
            with self.subTest(field=missing):
                values=dict(type=self.broker.DEAL_TYPE_BUY,
                            profit=0.,commission=0.,swap=0.,fee=0.)
                del values[missing]
                self.broker.deals=[N(**values)]
                with self.assertRaisesRegex(ValueError,'Daily breaker: invalid broker deal costs'):
                    self.exe._daily_loss_blocked(self.broker,self.cmd,self.broker.account)
                self.assertEqual(self.broker.sends,[])

    def test_nonfinite_commission_does_not_pass_broker_daily_breaker(self):
        self.broker.deals=[N(type=self.broker.DEAL_TYPE_SELL,
                             profit=0.,commission=math.nan,swap=0.,fee=0.)]
        with self.assertRaisesRegex(ValueError,'Daily breaker: invalid broker deal costs'):
            self.exe._daily_loss_blocked(self.broker,self.cmd,self.broker.account)
        self.assertEqual(self.broker.sends,[])


if __name__=='__main__':
    main()
