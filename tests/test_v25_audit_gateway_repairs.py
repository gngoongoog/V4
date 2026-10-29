"""Regression checks with fake brokers only; no MT5 or real account."""
from pathlib import Path
import json
import os
import sqlite3
import sys
import tempfile
import time
from types import SimpleNamespace as N
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'gateway'))
from broker_costs import booked_trading_net
from execution import BrokerExecutor, clock_ms
from institutional_controls import initialise_quality_table, record_execution_quality, quality_entry_blocked
from mt5_gateway import collect
from test_broker_execution import Broker, command
from test_gateway import FakeMT5


class AuditGatewayRepairs(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.ex = BrokerExecutor(Path(self.tmp.name) / 'journal.sqlite')
        self.b = Broker()
        self.cmd = command()

    def tearDown(self):
        self.ex.close()
        self.tmp.cleanup()

    def test_late_stop_or_account_change_prevents_send(self):
        for action in ('stop', 'account'):
            with self.subTest(action=action):
                self.cmd['id'] = action
                self.cmd.update(daily_breaker_enabled=True,daily_breaker_limit_percent=2.)
                calls = []
                running = [True]
                def history(*args):
                    calls.append(1)
                    if len(calls)==2:
                        if action == 'stop': running[0] = False
                        else: self.b.account.login = 99
                    return []
                self.b.history_deals_get = history
                out = self.ex.execute(self.b, self.cmd, lambda _: running[0])
                self.assertEqual(out['state'], 'cancelled')
                self.assertEqual(self.b.sends, [])
                self.b.account.login = 17

    def test_delayed_final_history_expires_the_old_quote_without_send(self):
        self.cmd.update(expires_ms=clock_ms()+100, max_quote_age_ms=100,
                        daily_breaker_enabled=True,daily_breaker_limit_percent=2.)
        calls=[]
        def history(*args):
            calls.append(1)
            if len(calls)==2: time.sleep(.15)
            return []
        self.b.history_deals_get=history
        out=self.ex.execute(self.b,self.cmd,lambda _:True)
        self.assertEqual(out['state'],'cancelled')
        self.assertEqual(self.b.sends,[])

    def test_account_switch_during_last_authorization_never_sends(self):
        calls=[]
        def authorize(_):
            calls.append(1)
            if len(calls)==3:self.b.account.login=99
            return True
        result=self.ex.execute(self.b,self.cmd,authorize)
        self.assertEqual(result['state'],'cancelled')
        self.assertEqual(self.b.sends,[])

    def test_slow_final_account_read_cannot_use_stale_core_permission(self):
        permissions=[];slowed=[False]
        def authorize(_):permissions.append(1);return True
        def slow(_):
            if len(permissions)==3 and not slowed[0]:
                slowed[0]=True;time.sleep(.04)
        self.b.on_account=slow
        result=self.ex.execute(self.b,self.cmd,authorize)
        self.assertEqual(result['state'],'cancelled')
        self.assertIn('authorization became stale',result['reason'])
        self.assertEqual(self.b.sends,[])

    def test_equity_drop_during_last_authorization_never_sends(self):
        self.cmd.update(risk_percent=1.,commission_per_lot=7.,slippage_ticks=2.)
        calls=[]
        def authorize(_):
            calls.append(1)
            if len(calls)==3:self.b.account.equity=1.
            return True
        result=self.ex.execute(self.b,self.cmd,authorize)
        self.assertEqual(result['state'],'cancelled')
        self.assertIn('equity or margin',result['reason'])
        self.assertEqual(self.b.sends,[])

    def test_daily_breaker_rechecks_equity_after_last_authorization(self):
        self.cmd.update(daily_breaker_enabled=True,daily_breaker_limit_percent=2.)
        calls=[]
        def authorize(_):
            calls.append(1)
            if len(calls)==3:self.b.account.equity=9700.
            return True
        result=self.ex.execute(self.b,self.cmd,authorize)
        self.assertEqual(result['state'],'cancelled')
        self.assertIn('Daily breaker reached',result['reason'])
        self.assertEqual(self.b.sends,[])
        self.assertEqual(self.ex.db.execute('SELECT COUNT(*) FROM daily_loss_locks').fetchone()[0],1)

    def test_malformed_ack_after_send_is_durable_unknown_and_not_retried(self):
        def malformed(request):
            self.b.sends.append(request)
            return N(retcode=10009,order=55,deal=77,volume=None,price=request['price'])
        self.b.order_send=malformed
        result=self.ex.execute(self.b,self.cmd,lambda _:True)
        self.assertEqual(result['state'],'unknown')
        saved=json.loads(self.ex.db.execute('SELECT result FROM sends WHERE id=?',(self.cmd['id'],)).fetchone()[0])
        self.assertEqual(saved['state'],'unknown')
        self.assertEqual(self.ex.execute(self.b,self.cmd,lambda _:True)['state'],'unknown')
        self.assertEqual(len(self.b.sends),1)

    def test_sniper_target_grid_is_exact_for_buy_and_sell(self):
        for side,bid,ask in ((1,1998.12,1998.13),(-1,1998.,1998.01)):
            self.cmd['id']=str(side)
            self.cmd['signal'].update(key='tick:test',side=side,target_distance=.13,stop_distance=1.)
            self.b.tick.bid=bid;self.b.tick.ask=ask;self.b.tick.time_msc=clock_ms()
            result=self.ex.execute(self.b,self.cmd,lambda _:True)
            self.assertEqual(result['state'],'filled',result)
            self.assertAlmostEqual(abs(result['request']['tp']-result['request']['price']),.13,places=8)

    def test_unrepresentable_sniper_stop_is_not_widened(self):
        self.b.info.trade_tick_size=.05;self.b.tick.bid=2000.05;self.b.tick.ask=2000.10
        self.cmd['signal'].update(key='tick:test',target_distance=.25,stop_distance=.82)
        result=self.ex.execute(self.b,self.cmd,lambda _:True)
        self.assertEqual(result['state'],'rejected')
        self.assertIn('SL was not widened',result['reason'])
        self.assertEqual(self.b.sends,[])

    def test_separate_commission_includes_loss_without_attributing_cash_transfer(self):
        self.b.deals=[N(type=0,profit=-4.,commission=0.,swap=0.,fee=0.),
                      N(type=7,profit=-3.,commission=0.,swap=0.,fee=0.),
                      N(type=2,profit=500.,commission=0.,swap=0.,fee=0.)]
        self.b.account.balance=93.;self.b.account.equity=93.
        self.cmd.update(daily_breaker_enabled=True,daily_breaker_limit_percent=5.)
        self.assertEqual(booked_trading_net(self.b.deals,self.b),(-7.,1))
        self.assertTrue(self.ex._daily_loss_blocked(self.b,self.cmd,self.b.account))

    def test_zero_fill_cancel_requires_exact_terminal_order_evidence(self):
        self.b.code=10008;self.b.reply_deal=0;self.b.ORDER_STATE_CANCELED=2
        self.assertEqual(self.ex.execute(self.b,self.cmd,lambda _:True)['state'],'placed')
        row=N(ticket=55,state=2,volume_initial=.1,volume_current=.05,symbol='XAUUSD',magic=410601)
        self.b.history_orders_get=lambda **kw:[row]
        self.assertEqual(self.ex.execute(self.b,self.cmd,lambda _:True)['state'],'placed')
        row.volume_current=.1
        result=self.ex.execute(self.b,self.cmd,lambda _:True)
        self.assertEqual(result['state'],'cancelled')
        self.assertEqual(result['broker_terminal_order']['ticket'],55)
        self.assertEqual(len(self.b.sends),1)

    def test_bad_history_price_stays_unknown_and_can_recover_later(self):
        self.b.raise_send=True
        self.assertEqual(self.ex.execute(self.b,self.cmd,lambda _:True)['state'],'unknown')
        self.b.raise_send=False
        deal=N(ticket=701,magic=self.cmd['magic'],symbol=self.cmd['symbol'],volume=.1,
               type=0,entry=0,position_id=700,comment='BT:'+self.cmd['id'][:20],price=None,time_msc=clock_ms())
        self.b.deals=[deal]
        self.assertEqual(self.ex.execute(self.b,self.cmd,lambda _:True)['state'],'unknown')
        deal.price=2000.10
        self.assertEqual(self.ex.execute(self.b,self.cmd,lambda _:True)['state'],'filled')
        self.assertEqual(len(self.b.sends),1)

    def test_existing_close_order_remains_unknown_and_blocks_new_close_intent(self):
        self.b.positions=[N(ticket=41,magic=410601,symbol='XAUUSD',volume=.1,type=0)]
        self.b.orders=[N(ticket=78,magic=410601,symbol='XAUUSD')]
        self.b.code=10039;self.b.reply_deal=0
        self.cmd.update(id='one',kind='close',position=41)
        self.assertEqual(self.ex.execute(self.b,self.cmd,lambda _:True)['state'],'unknown')
        self.cmd['id']='two'
        self.assertEqual(self.ex.execute(self.b,self.cmd,lambda _:True)['state'],'cancelled')
        self.assertEqual(len(self.b.sends),1)

    def test_reconciled_quality_updates_same_row_without_inflation(self):
        db=sqlite3.connect(':memory:');initialise_quality_table(db)
        record_execution_quality(db,self.cmd,dict(state='unknown',send_ms=10),1000)
        record_execution_quality(db,self.cmd,dict(state='filled',retcode=10009,send_ms=10),5000)
        with patch.dict(os.environ,{'BRAINTRADE_QUALITY_CIRCUIT':'1'}):
            self.assertIsNone(quality_entry_blocked(db,self.cmd,5000))
        self.assertEqual(db.execute('SELECT state,recorded_ms FROM execution_quality').fetchall(),[('filled',1000)])
        db.close()

    def test_margin_display_uses_more_conservative_broker_side(self):
        mt=FakeMT5();mt.ORDER_TYPE_BUY=0;mt.ORDER_TYPE_SELL=1
        mt.order_calc_margin=lambda side,*args:10. if side==0 else 20.
        self.assertEqual(collect(mt,'XAUUSD')['minimum_lot_margin'],20.)
        mt.order_calc_margin=lambda side,*args:10. if side==0 else None
        self.assertIsNone(collect(mt,'XAUUSD')['minimum_lot_margin'])

    def test_periodic_history_rescan_recovers_late_receipt_without_rewinding_cursor(self):
        base=1_800_000_000_000
        self.b.account.login=17
        self.b.deals=[]
        ranges=[];synced=[]
        deal=N(ticket=101,position_id=55,magic=410600,symbol='XAUUSD',entry=1,type=1,
               time_msc=base-1000,volume=.1,price=2000.,profit=1.,commission=0.,swap=0.,fee=0.,reason=0)
        visible=[False]
        def history(start,end):
            lo,hi=int(start.timestamp()*1000),int(end.timestamp()*1000);ranges.append((lo,hi))
            return [deal] if visible[0] and lo<=deal.time_msc<=hi else []
        self.b.history_deals_get=history
        def api(path,payload=None):
            self.assertEqual(path,'/v1/execution/sync');synced.extend(d['ticket'] for d in payload['deals']);return {'ok':True}
        identity=dict(login=17,server='Demo',currency='USD',trade_mode=0)
        work=dict(command=None,history_from_ms=base-120000,history_rescan_from_ms=base-600000,
                  history_scope='account-and-symbol',account=identity,poll_ms=250)
        with patch('execution.clock_ms',return_value=base):self.ex.cycle(self.b,api,work)
        visible[0]=True;self.ex.next_history=0;work['history_from_ms']=base+30_000
        with patch('execution.clock_ms',return_value=base+301_000):self.ex.cycle(self.b,api,work)
        self.assertEqual(synced,[101])
        self.assertEqual(ranges[-1][0],base-600000)


if __name__ == '__main__': unittest.main()
