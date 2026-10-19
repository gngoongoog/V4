from __future__ import annotations
import copy
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace as N
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'gateway'))
from execution import BrokerExecutor,clock_ms

class Broker:
    TRADE_ACTION_DEAL=1; ORDER_TYPE_BUY=0; ORDER_TYPE_SELL=1; POSITION_TYPE_BUY=0
    ORDER_TIME_GTC=0; ORDER_FILLING_FOK=0; ORDER_FILLING_IOC=1; ORDER_FILLING_RETURN=2
    ACCOUNT_MARGIN_MODE_RETAIL_HEDGING=2; SYMBOL_TRADE_EXECUTION_MARKET=2
    def __init__(self):
        self.account=N(margin_free=10000.,login=17,server='Demo',currency='USD',trade_mode=0,margin_mode=2,trade_allowed=True,trade_expert=True)
        self.terminal=N(connected=True,trade_allowed=True,tradeapi_disabled=False)
        self.info=N(point=.01,trade_tick_size=.01,volume_min=.01,volume_step=.01,volume_max=100.,filling_mode=3,trade_exemode=2,trade_stops_level=5)
        self.tick=N(bid=2000.,ask=2000.10,time_msc=clock_ms())
        self.positions=[];self.orders=[];self.deals=[];self.sends=[];self.code=10009;self.raise_send=False;self.check_code=0;self.on_check=None
        self.account_reads=0;self.on_account=None
    def account_info(self):
        self.account_reads+=1
        if self.on_account:self.on_account(self.account_reads)
        return self.account
    def terminal_info(self):return self.terminal
    def symbol_info(self,*a):return self.info
    def symbol_info_tick(self,*a):return self.tick
    def positions_get(self,**kw):return self.positions
    def orders_get(self,**kw):return self.orders
    def history_deals_get(self,*a):return self.deals
    def order_calc_margin(self,side,symbol,volume,price):return volume*1000.
    def order_check(self,r):
        if self.on_check:self.on_check()
        return N(retcode=self.check_code,comment='check')
    def order_send(self,r):
        self.sends.append(r)
        if self.raise_send:raise RuntimeError('network failure after submit')
        return N(retcode=self.code,comment='broker',order=55,deal=77,volume=r['volume'],price=r['price'])

def command():
    now=clock_ms()
    return dict(id='a'*32,login=17,server='Demo',currency='USD',target='demo',symbol='XAUUSD',magic=410601,kind='open',position=0,lots=.1,deviation_points=20,created_ms=now,expires_ms=now+30000,signal=dict(side=1,stop_distance=1.,target_distance=2.),state='queued')

class ExecutionTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.path=Path(self.temp.name)/'journal.sqlite';self.ex=BrokerExecutor(self.path);self.mt=Broker();self.cmd=command()
    def tearDown(self):self.ex.close();self.temp.cleanup()
    def send(self):return self.ex.execute(self.mt,self.cmd,lambda _:True)
    def test_sniper_identity_and_reserved_adaptive_identity(self):
        self.cmd['magic']=410616
        self.assertEqual(self.send()['state'],'rejected')
        self.assertFalse(self.mt.sends)
        self.cmd['id']='sniper-specialist';self.cmd['magic']=410617
        self.cmd['signal']['key']='tick:123'
        self.assertEqual(self.send()['state'],'filled')
        self.assertEqual(len(self.mt.sends),1)

    def test_flexible_100_dollar_demo_downsizes_before_one_real_send(self):
        self.mt.account.margin_free=100.
        self.cmd.update(demo_flexible=True,commission_per_lot=7.,lots=.5)
        result=self.send()
        self.assertEqual(result['state'],'filled')
        self.assertAlmostEqual(result['sizing']['sent'],.09)
        self.assertLessEqual(result['sizing']['estimated_margin_plus_commission'],100.)
        self.assertEqual(len(self.mt.sends),1)
        self.send();self.assertEqual(len(self.mt.sends),1)
    def test_flexible_minimum_is_not_invented_and_zero_margin_never_sends(self):
        self.cmd.update(demo_flexible=True,commission_per_lot=7.)
        self.mt.account.margin_free=5.
        result=self.send();self.assertEqual(result['state'],'rejected')
        self.assertIn('broker minimum',result['reason']);self.assertFalse(self.mt.sends)
        self.mt.account.margin_free=100.;self.cmd['lots']=.001
        result=self.send();self.assertIn('not increased',result['reason']);self.assertFalse(self.mt.sends)
    def test_flexible_volume_rounds_down_and_is_demo_only(self):
        self.cmd.update(demo_flexible=True,commission_per_lot=7.,lots=.015)
        self.assertEqual(self.send()['request']['volume'],.01)
        self.cmd['id']='new-live';self.cmd['target']='live';self.mt.account.trade_mode=2
        self.assertEqual(self.send()['state'],'rejected')
        self.assertEqual(len(self.mt.sends),1)
    def test_flexible_margin_estimate_failure_does_not_guess(self):
        self.cmd.update(demo_flexible=True)
        self.mt.order_calc_margin=lambda *args:None
        self.assertIn('estimate unavailable',self.send()['reason']);self.assertFalse(self.mt.sends)
    def test_filled_once_even_after_restart_and_duplicate_delivery(self):
        r=self.send();self.assertEqual(r['state'],'filled');self.assertEqual(self.mt.sends[0]['type_filling'],0);self.assertLess(self.mt.sends[0]['sl'],self.mt.tick.bid);self.assertGreater(self.mt.sends[0]['tp'],self.mt.tick.ask)
        self.ex.close();self.ex=BrokerExecutor(self.path);self.assertEqual(self.send()['state'],'filled');self.assertEqual(len(self.mt.sends),1)
    def test_timeout_never_retries_and_later_history_reconciles(self):
        self.mt.raise_send=True;self.assertEqual(self.send()['state'],'unknown');self.mt.raise_send=False;self.assertEqual(self.send()['state'],'unknown');self.assertEqual(len(self.mt.sends),1)
        self.mt.deals=[N(ticket=77,magic=self.cmd['magic'],symbol='XAUUSD',volume=.1,comment='BT:'+self.cmd['id'][:20])];self.assertEqual(self.send()['state'],'filled');self.assertEqual(len(self.mt.sends),1)
    def test_partial_close_reconciliation_retains_submitted_volume_after_crash(self):
        self.cmd.update(kind='close',position=1)
        self.mt.positions=[N(ticket=1,magic=self.cmd['magic'],symbol='XAUUSD',volume=.1,type=0)]
        self.mt.raise_send=True;self.assertEqual(self.send()['state'],'unknown')
        self.ex.db.execute('UPDATE sends SET result=NULL');self.ex.db.commit()
        self.ex.close();self.ex=BrokerExecutor(self.path)
        self.mt.deals=[N(ticket=77,magic=self.cmd['magic'],symbol='XAUUSD',volume=.04,comment='BT:'+self.cmd['id'][:20])]
        result=self.send();self.assertEqual(result['state'],'partial');self.assertEqual(result['request']['volume'],.1)
        self.assertEqual(result['deal'],77);self.assertEqual(len(self.mt.sends),1)
    def test_crash_between_journal_and_send_stays_unknown(self):
        import json
        self.ex.db.execute('INSERT INTO sends VALUES(?,?,NULL,?)',(self.cmd['id'],json.dumps(self.cmd),clock_ms()));self.ex.db.commit();self.assertEqual(self.send()['state'],'unknown');self.assertFalse(self.mt.sends)
    def test_partial_and_placed_are_not_topped_up(self):
        for code,state in [(10010,'partial'),(10008,'placed'),(10012,'unknown'),(10031,'unknown')]:
            self.cmd['id']=str(code)*6;self.mt.code=code;self.assertEqual(self.send()['state'],state);count=len(self.mt.sends);self.send();self.assertEqual(len(self.mt.sends),count)
    def test_demo_does_not_send_to_real_or_changed_account(self):
        for field,value in [('trade_mode',2),('login',99),('server','Different'),('currency','EUR')]:
            before=getattr(self.mt.account,field);setattr(self.mt.account,field,value);self.assertEqual(self.send()['state'],'rejected');setattr(self.mt.account,field,before)
        self.assertFalse(self.mt.sends)
    def test_live_requires_matching_live_binding(self):
        self.cmd['target']='live';self.assertEqual(self.send()['state'],'rejected');self.mt.account.trade_mode=2;self.assertEqual(self.send()['state'],'filled')
    def test_account_switch_during_check_and_stop_before_send(self):
        self.mt.on_check=lambda:setattr(self.mt.account,'login',88);self.assertEqual(self.send()['state'],'rejected');self.assertFalse(self.mt.sends)
        self.mt.on_check=None;self.mt.account.login=17;self.assertEqual(self.ex.execute(self.mt,self.cmd,lambda _:False)['state'],'cancelled');self.assertFalse(self.mt.sends)
    def test_stop_after_first_authorization_is_caught_after_durable_journal(self):
        calls=0
        def authorize(_):
            nonlocal calls
            calls+=1
            return calls==1
        result=self.ex.execute(self.mt,self.cmd,authorize)
        self.assertEqual(result['state'],'cancelled')
        self.assertIn('after durable journal',result['reason'])
        self.assertEqual(calls,2)
        self.assertFalse(self.mt.sends)
        # The terminal cancellation is durable too: duplicate delivery cannot send it later.
        self.assertEqual(self.send()['state'],'cancelled')
        self.assertFalse(self.mt.sends)
    def test_account_switch_after_journal_is_caught_before_order_send(self):
        def switch_on_final_read(read):
            if read==3:self.mt.account.login=88
        self.mt.on_account=switch_on_final_read
        result=self.send()
        self.assertEqual(result['state'],'cancelled')
        self.assertIn('after durable journal',result['reason'])
        self.assertEqual(self.mt.account_reads,3)
        self.assertFalse(self.mt.sends)
    def test_volume_spread_stops_and_quote_contract(self):
        self.cmd['lots']=.015;self.assertEqual(self.send()['state'],'rejected');self.cmd['lots']=.1
        self.mt.tick.ask=2002.;self.assertEqual(self.send()['state'],'rejected');self.mt.tick.ask=2000.10
        self.mt.tick.time_msc=clock_ms()-5000;self.assertEqual(self.send()['state'],'rejected');self.assertFalse(self.mt.sends)
    def test_netting_and_hedging_ownership(self):
        self.mt.positions=[N(ticket=1,magic=999,symbol='XAUUSD')];self.mt.account.margin_mode=0;self.assertEqual(self.send()['state'],'rejected');self.mt.account.margin_mode=2;self.assertEqual(self.send()['state'],'filled')
    def test_close_only_owned_ticket_and_actual_partial_volume(self):
        self.cmd.update(kind='close',position=1);self.mt.positions=[N(ticket=1,magic=999,symbol='XAUUSD',volume=.05,type=0)];self.assertEqual(self.send()['state'],'closed');self.assertFalse(self.mt.sends)
        self.mt.positions[0].magic=self.cmd['magic'];self.assertEqual(self.send()['state'],'filled');req=self.mt.sends[0];self.assertEqual(req['volume'],.05);self.assertEqual(req['position'],1);self.assertEqual(req['type'],1)
    def test_filling_policy_and_broker_rejection(self):
        self.mt.info.filling_mode=2;self.mt.code=10019;self.assertEqual(self.send()['state'],'rejected');self.assertEqual(self.mt.sends[0]['type_filling'],1)
    def test_missing_journal_unknown_never_becomes_new_order(self):
        self.cmd['state']='unknown';self.assertEqual(self.send()['state'],'unknown');self.assertFalse(self.mt.sends)
    def test_sniper_tick_fast_path_skips_broker_check_and_reports_timings(self):
        self.cmd['signal']['key']='tick:123456789'
        self.mt.check_code=10019
        self.mt.on_check=lambda: self.fail('Sniper fast path must not call order_check')
        result=self.send()
        self.assertEqual(result['state'],'filled')
        self.assertTrue(result['fast_path'])
        self.assertTrue(result['order_check_skipped'])
        self.assertEqual(result['check_ms'],0.0)
        for key in ('queue_to_gateway_ms','authorize_ms','journal_ms','post_journal_authorize_ms','post_journal_account_ms','pre_send_ms','queue_to_send_ms','send_ms','total_ms'):
            self.assertIn(key,result)
            self.assertGreaterEqual(result[key],0)
        self.assertEqual(len(self.mt.sends),1)
    def test_sniper_fast_path_reprices_stop_to_current_broker_floor(self):
        self.cmd['signal'].update(key='tick:geometry',stop_distance=.02,target_distance=.02)
        result=self.send()
        self.assertEqual(result['state'],'filled')
        self.assertEqual(result['pricing']['policy'],'sniper_send_geometry_v1')
        self.assertAlmostEqual(result['pricing']['broker_minimum_distance'],.06,places=9)
        self.assertAlmostEqual(result['pricing']['stop_distance'],.16,places=9)
        request=self.mt.sends[0]
        self.assertGreaterEqual((self.mt.tick.bid-request['sl'])+1e-9,.05)
        self.assertGreaterEqual((request['tp']-self.mt.tick.bid)+1e-9,.05)
    def test_regular_path_does_not_silently_widen_invalid_geometry(self):
        self.cmd['signal'].update(stop_distance=.02,target_distance=.02)
        result=self.send()
        self.assertEqual(result['state'],'rejected')
        self.assertFalse(result['fast_path'])
        self.assertFalse(self.mt.sends)
    def test_sniper_fast_path_rechecks_identity_after_authorization(self):
        self.cmd['signal']['key']='tick:987654321'
        def authorize(_):
            self.mt.account.login=88
            return True
        result=self.ex.execute(self.mt,self.cmd,authorize)
        self.assertEqual(result['state'],'rejected')
        self.assertIn('Account changed',result['reason'])
        self.assertFalse(self.mt.sends)
    def test_regular_path_still_requires_broker_order_check(self):
        self.mt.check_code=10019
        result=self.send()
        self.assertEqual(result['state'],'rejected')
        self.assertFalse(result['fast_path'])
        self.assertFalse(self.mt.sends)

if __name__=='__main__':unittest.main()