from __future__ import annotations
import copy
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace as N
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'gateway'))
from execution import BrokerExecutor,clock_ms,round_trip_commission,size_to_stop_risk

class Broker:
    TRADE_ACTION_DEAL=1; ORDER_TYPE_BUY=0; ORDER_TYPE_SELL=1; POSITION_TYPE_BUY=0
    ORDER_TIME_GTC=0; ORDER_FILLING_FOK=0; ORDER_FILLING_IOC=1; ORDER_FILLING_RETURN=2
    ACCOUNT_MARGIN_MODE_RETAIL_HEDGING=2; SYMBOL_TRADE_EXECUTION_MARKET=2
    DEAL_TYPE_BUY=0; DEAL_TYPE_SELL=1; DEAL_ENTRY_IN=0; DEAL_ENTRY_OUT=1; DEAL_ENTRY_OUT_BY=3
    def __init__(self):
        self.account=N(balance=10000.,equity=10000.,margin_free=10000.,login=17,server='Demo',currency='USD',trade_mode=0,margin_mode=2,trade_allowed=True,trade_expert=True)
        self.terminal=N(connected=True,trade_allowed=True,tradeapi_disabled=False)
        self.info=N(point=.01,trade_tick_size=.01,volume_min=.01,volume_step=.01,volume_max=100.,filling_mode=3,trade_exemode=2,trade_stops_level=5,trade_tick_value_loss=1.0)
        self.tick=N(bid=2000.,ask=2000.10,time_msc=clock_ms())
        self.positions=[];self.orders=[];self.deals=[];self.sends=[];self.code=10009;self.raise_send=False;self.check_code=0;self.on_check=None
        self.account_reads=0;self.on_account=None;self.reply_deal=77
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
    def order_calc_profit(self,side,symbol,volume,entry,close):
        return (close-entry if side==self.ORDER_TYPE_BUY else entry-close)*100.*volume
    def order_check(self,r):
        if self.on_check:self.on_check()
        return N(retcode=self.check_code,comment='check')
    def order_send(self,r):
        self.sends.append(r)
        if self.raise_send:raise RuntimeError('network failure after submit')
        return N(retcode=self.code,comment='broker',order=55,deal=self.reply_deal,volume=r['volume'],price=r['price'])

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

    def test_exact_commission_schedule_roundtrip_and_no_commission_account(self):
        for lots, fee in ((0.01,0.07),(0.10,0.70),(1.0,7.0)):
            self.assertAlmostEqual(round_trip_commission(7.,lots),fee)
            self.assertEqual(round_trip_commission(0.,lots),0.)
        for rate,lots in ((float('nan'),.1),(-7.,.1),(7.,0),(7.,-1.)):
            with self.assertRaises(ValueError):
                round_trip_commission(rate,lots)

        self.mt.account.margin_free=2000.
        for i,(lots,fee) in enumerate(((0.01,0.07),(.1,.7),(1.,7.))):
            self.cmd.update(id=f'commission-normal-{i}',lots=lots,
                            demo_flexible=True,commission_per_lot=7.)
            result=self.send()
            self.assertEqual(result['state'],'filled',result)
            self.assertAlmostEqual(result['request']['volume'],lots)
            self.assertAlmostEqual(result['sizing']['estimated_round_trip_commission'],fee)
            self.assertAlmostEqual(result['sizing']['commission_round_trip_per_lot'],7.)

        self.cmd.update(id='commission-free',lots=.1,
                        demo_flexible=True,commission_per_lot=0.)
        result=self.send()
        self.assertEqual(result['state'],'filled',result)
        self.assertEqual(result['sizing']['estimated_round_trip_commission'],0.)
        self.assertAlmostEqual(result['sizing']['estimated_margin_plus_commission'],
                               result['sizing']['estimated_margin'])

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
    def test_real_mode_fills_only_matching_real_broker_after_core_authorizes(self):
        # This is a simulated broker, never a funded MT5 terminal.
        self.mt.account.trade_mode=2
        self.cmd['target']='live'
        self.cmd.update(risk_percent=0.25,commission_per_lot=7.,slippage_ticks=2.)
        self.cmd['id']='real-accepted-01'
        rejected=self.ex.execute(self.mt,self.cmd,lambda _:False)
        self.assertEqual(rejected['state'],'cancelled',
                         'Core final authorization refusal is a cancellation, never a broker fill')
        self.assertFalse(self.mt.sends)
        self.cmd['id']='real-accepted-02'
        accepted=self.ex.execute(self.mt,self.cmd,lambda _:True)
        self.assertEqual(accepted['state'],'filled',accepted)
        self.assertEqual(len(self.mt.sends),1)
        self.assertAlmostEqual(self.mt.sends[0]['volume'],.1)
        self.assertEqual(self.send()['state'],'filled')
        self.assertEqual(len(self.mt.sends),1,'intent remains idempotent')
        self.cmd['id']='real-mismatched-demo'
        self.mt.account.trade_mode=0
        self.assertEqual(self.send()['state'],'rejected')
        self.assertEqual(len(self.mt.sends),1,'real order may never go to a demo account')

    def test_risk_sizing_is_lot_cap_not_lot_increase_and_commission_is_proportional(self):
        self.mt.account.equity=1000.
        self.cmd.update(id='risk-lot-cap',risk_percent=0.5,
                        commission_per_lot=7.,slippage_ticks=2.,lots=.1)
        result=self.send()
        self.assertEqual(result['state'],'filled',result)
        # SL is one gold price unit: $100 per 1.00 lot on this mock
        # + $7 round-trip fee + $4 two-sided slippage reserve per lot.
        self.assertAlmostEqual(result['risk_sizing']['broker_stop_loss_per_lot'],100.)
        self.assertAlmostEqual(result['risk_sizing']['commission_round_trip_per_lot'],7.)
        self.assertAlmostEqual(result['risk_sizing']['slippage_cushion_per_lot'],4.)
        self.assertAlmostEqual(result['risk_sizing']['risk_budget'],5.)
        self.assertAlmostEqual(result['request']['volume'],.04)
        self.assertLessEqual(result['risk_sizing']['estimated_stop_loss_including_costs'],5.)
        self.assertAlmostEqual(self.mt.sends[0]['volume'],.04)

    def test_risk_sizing_rejects_broker_minimum_without_fabricating_lot(self):
        self.mt.account.equity=100.
        self.cmd.update(id='risk-min',risk_percent=0.25,commission_per_lot=7.,
                        slippage_ticks=2.,lots=.01)
        result=self.send()
        self.assertEqual(result['state'],'rejected')
        self.assertIn('minimum 0.01 lot needs',result['reason'])
        self.assertIn('of equity',result['reason'])
        self.assertIn('Leverage changes margin, not stop loss.',result['reason'])
        self.assertEqual(self.mt.sends,[])

    def test_micro_demo_opt_in_enables_costed_minimum_without_live_override(self):
        self.mt.account.equity=100.
        self.mt.account.margin_free=100.
        self.cmd.update(id='micro-no-optin',lots=.01,demo_flexible=True,
                        risk_percent=2.0,commission_per_lot=7.,
                        slippage_ticks=2.)
        self.cmd['signal']['stop_distance']=3.
        blocked=self.send()
        self.assertEqual(blocked['state'],'rejected',blocked)
        self.assertEqual(len(self.mt.sends),0)
        self.cmd.update(id='micro-demo-accepted',risk_percent=4.0,
                        micro_demo_risk_opt_in=True)
        accepted=self.send()
        self.assertEqual(accepted['state'],'filled',accepted)
        self.assertAlmostEqual(accepted['request']['volume'],.01)
        self.assertAlmostEqual(accepted['risk_sizing']['estimated_stop_loss_including_costs'],3.11)
        self.assertEqual(len(self.mt.sends),1)
        self.cmd.update(id='micro-demo-no-consent',micro_demo_risk_opt_in=False)
        rejected=self.send()
        self.assertEqual(rejected['state'],'rejected')
        self.assertEqual(len(self.mt.sends),1)
        self.cmd.update(id='micro-real-reject',target='live',micro_demo_risk_opt_in=True)
        self.mt.account.trade_mode=2
        rejected=self.send()
        self.assertEqual(rejected['state'],'rejected')
        self.assertEqual(len(self.mt.sends),1)
    def test_real_micro_temporary_risk_until_recovery_is_durable(self):
        self.mt.account.trade_mode=2
        self.mt.account.balance=50.
        self.mt.account.equity=50.
        self.mt.account.margin_free=50.
        self.cmd.update(target='live',id='real-micro-without-switch',lots=.01,
                        risk_percent=10.,commission_per_lot=7.,slippage_ticks=2.,
                        micro_real_recovery_equity=500.)
        self.cmd['signal']['stop_distance']=3.
        disabled=self.send()
        self.assertEqual(disabled['state'],'rejected',disabled)
        self.assertEqual(len(self.mt.sends),0)

        self.cmd.update(id='real-micro-enabled',micro_real_temporary=True)
        permitted=self.send()
        self.assertEqual(permitted['state'],'filled',permitted)
        self.assertAlmostEqual(permitted['risk_sizing']['estimated_stop_loss_including_costs'],3.11)
        self.assertAlmostEqual(permitted['risk_sizing']['risk_budget'],5.)
        self.assertEqual(len(self.mt.sends),1)

        self.mt.account.balance=500.
        self.mt.account.equity=500.
        self.mt.account.margin_free=500.
        self.cmd['id']='real-micro-hit-milestone'
        reached=self.send()
        self.assertEqual(reached['state'],'rejected',reached)
        self.assertIn('recovery milestone',reached['reason'])
        self.mt.account.balance=50.
        self.mt.account.equity=50.
        self.mt.account.margin_free=50.
        self.cmd['id']='real-micro-fell-below'
        again=self.send()
        self.assertEqual(again['state'],'rejected',again)
        self.assertIn('recovery milestone',again['reason'])
        self.assertEqual(len(self.mt.sends),1)
        self.ex.close()
        self.ex=BrokerExecutor(self.path)
        self.cmd['id']='real-micro-after-restart'
        restarted=self.send()
        self.assertEqual(restarted['state'],'rejected',restarted)
        self.assertEqual(len(self.mt.sends),1)

    def test_real_micro_recovery_during_validation_cancels_before_send(self):
        self.mt.account.trade_mode=2
        self.mt.account.balance=50.
        self.mt.account.equity=50.
        self.mt.account.margin_free=50.
        self.cmd.update(target='live',id='real-micro-equity-jump',lots=.01,
                        risk_percent=10.,commission_per_lot=7.,slippage_ticks=2.,
                        micro_real_recovery_equity=500.,micro_real_temporary=True)
        self.cmd['signal']['stop_distance']=3.
        def jump(n):
            if n>=3:
                self.mt.account.equity=501.
                self.mt.account.margin_free=501.
        self.mt.on_account=jump
        out=self.send()
        self.assertIn(out['state'],('cancelled','rejected'),out)
        self.assertEqual(len(self.mt.sends),0)

    def test_real_margin_preflight_is_mandatory_in_fast_path(self):
        self.mt.account.trade_mode=2
        self.mt.account.balance=50.
        self.mt.account.equity=50.
        self.mt.account.margin_free=5.
        self.cmd.update(target='live',id='micro-fast-margin',lots=.01,
                        risk_percent=10.,commission_per_lot=7.,slippage_ticks=2.,
                        micro_real_recovery_equity=500.,micro_real_temporary=True)
        self.cmd['signal'].update(key='tick:77',stop_distance=3.)
        rejected=self.send()
        self.assertEqual(rejected['state'],'rejected',rejected)
        self.assertIn('REAL margin preflight',rejected['reason'])
        self.assertEqual(self.mt.sends,[])

        self.mt.account.margin_free=50.
        self.mt.account_reads=0
        def decline(n):
            if n>=3:self.mt.account.margin_free=1.
        self.mt.on_account=decline
        self.cmd['id']='micro-fast-margin-declined'
        cancelled=self.send()
        self.assertEqual(cancelled['state'],'cancelled',cancelled)
        self.assertIn('free margin dropped',cancelled['reason'])
        self.assertEqual(self.mt.sends,[])

    def test_daily_breaker_real_locked_across_symbols_and_demo_switch_is_independent(self):
        self.mt.account.trade_mode=2
        self.mt.account.balance=50.
        self.mt.account.equity=50.
        self.mt.account.margin_free=50.
        self.mt.deals=[N(type=self.mt.DEAL_TYPE_SELL,profit=-3.,commission=0.,swap=0.,fee=0.)]
        self.cmd.update(target='live',id='daily-loss-reached',lots=.01,
                        risk_percent=10.,micro_real_temporary=True,
                        micro_real_recovery_equity=500.,
                        commission_per_lot=7.,slippage_ticks=2.,
                        daily_breaker_enabled=True,daily_breaker_limit_percent=2.)
        self.cmd['signal']['stop_distance']=3.
        out=self.send()
        self.assertEqual(out['state'],'rejected',out)
        self.assertIn('Daily breaker',out['reason'])
        self.assertEqual(self.mt.sends,[])
        self.mt.deals=[]
        self.cmd['id']='daily-loss-latched'
        latched=self.send()
        self.assertEqual(latched['state'],'rejected',latched)
        self.assertIn('Daily breaker',latched['reason'])
        # Explicitly OFF overrides only this gate; preserved lock returns ON.
        self.cmd.update(id='daily-loss-disabled',daily_breaker_enabled=False)
        allowed=self.send()
        self.assertEqual(allowed['state'],'filled',allowed)
        self.assertEqual(len(self.mt.sends),1)
        self.ex.close();self.ex=BrokerExecutor(self.path)
        self.cmd.update(id='daily-loss-reenabled',daily_breaker_enabled=True,symbol='EURUSD')
        out=self.send()
        self.assertEqual(out['state'],'rejected',out)
        self.assertIn('Daily breaker',out['reason'])
        self.assertEqual(len(self.mt.sends),1)

        # Demo disabled by default; loss-history outages do not block practice.
        self.mt.account.trade_mode=0
        self.cmd.update(id='daily-demo-open',target='demo',symbol='XAUUSD',
                        risk_percent=2.,micro_real_temporary=False,
                        daily_breaker_enabled=False)
        self.cmd['signal']['stop_distance']=.5
        self.mt.history_deals_get=lambda *a:None
        self.mt.account.equity=50.
        self.mt.account.balance=50.
        allowed=self.send()
        self.assertEqual(allowed['state'],'filled',allowed)
        self.assertEqual(len(self.mt.sends),2)
        self.cmd.update(id='daily-demo-enforced',daily_breaker_enabled=True)
        refused=self.send()
        self.assertEqual(refused['state'],'rejected',refused)
        self.assertIn('history/positions unavailable',refused['reason'])
        self.assertEqual(len(self.mt.sends),2)

    def test_daily_breaker_final_equity_drop_blocks_after_journal(self):
        self.mt.account.trade_mode=2
        self.mt.account.balance=10000.
        self.mt.account.equity=10000.
        self.cmd.update(target='live',id='daily-equity-drops',risk_percent=.25,
                        commission_per_lot=7.,slippage_ticks=2.,
                        daily_breaker_enabled=True,daily_breaker_limit_percent=2.)
        self.mt.on_account=lambda n: setattr(self.mt.account,'equity',9500.) if n>=3 else None
        out=self.send()
        self.assertEqual(out['state'],'cancelled',out)
        self.assertIn('Daily breaker',out['reason'])
        self.assertEqual(len(self.mt.sends),0)

    def test_risk_sizing_zero_commission_and_unavailable_broker_profit_fail_closed(self):
        self.mt.account.equity=1000.
        self.cmd.update(id='risk-free-fee',risk_percent=0.5,
                        commission_per_lot=0.,slippage_ticks=0.,lots=.1)
        result=self.send()
        self.assertEqual(result['state'],'filled',result)
        self.assertEqual(result['risk_sizing']['commission_round_trip_per_lot'],0.)
        self.assertAlmostEqual(result['request']['volume'],.05)
        self.cmd['id']='risk-no-profit-estimate'
        self.mt.order_calc_profit=lambda *args:None
        rejected=self.send()
        self.assertEqual(rejected['state'],'rejected')
        self.assertIn('broker loss estimate unavailable',rejected['reason'])
        self.assertEqual(len(self.mt.sends),1)

    def test_real_missing_risk_budget_and_bad_equity_are_rejected_before_send(self):
        self.mt.account.trade_mode=2
        self.cmd.update(id='risk-real-missing',target='live')
        result=self.send()
        self.assertEqual(result['state'],'rejected')
        self.assertIn('no Core risk budget',result['reason'])
        self.cmd.update(id='risk-real-bad-equity',risk_percent=0.25,
                        commission_per_lot=7.,slippage_ticks=2.)
        self.mt.account.equity=float('nan')
        result=self.send()
        self.assertEqual(result['state'],'rejected')
        self.assertTrue('positive broker equity required' in result['reason'] or 'Daily breaker: valid balance, equity' in result['reason'])
        self.assertFalse(self.mt.sends)

    def test_broker_equity_drop_after_durable_journal_cancels_send(self):
        self.mt.account.equity=1000.
        self.cmd.update(id='risk-equity-final',target='demo',risk_percent=0.5,
                        commission_per_lot=7.,slippage_ticks=2.,lots=.1)
        def drop_equity(reads):
            if reads==3:
                self.mt.account.equity=200.
        self.mt.on_account=drop_equity
        result=self.send()
        self.assertEqual(result['state'],'cancelled',result)
        self.assertIn('equity fell',result['reason'])
        self.assertFalse(self.mt.sends)
        self.assertEqual(self.send()['state'],'cancelled',
                         'no duplicate send or automatic retry after Core journal')

    def test_unavailable_final_broker_equity_persists_explicit_no_send_result(self):
        self.mt.account.equity=1000.
        self.cmd.update(id='risk-equity-null-final',risk_percent=0.5,
                        commission_per_lot=7.,slippage_ticks=2.,lots=.1)
        self.mt.on_account=lambda reads: setattr(self.mt.account,'equity',None) if reads==3 else None
        result=self.send()
        self.assertEqual(result['state'],'cancelled',result)
        self.assertFalse(self.mt.sends)
        self.assertEqual(self.send()['state'],'cancelled')

    def test_demo_margin_shrink_followed_by_stop_risk_never_exceeds_either_cap(self):
        self.mt.account.equity=1000.
        self.mt.account.margin_free=90.
        self.cmd.update(id='risk-and-margin',lots=.5,demo_flexible=True,
                        risk_percent=0.5,commission_per_lot=7.,slippage_ticks=2.)
        result=self.send()
        self.assertEqual(result['state'],'filled',result)
        self.assertAlmostEqual(result['sizing']['sent'],.08)
        self.assertAlmostEqual(result['risk_sizing']['sent'],.04)
        self.assertAlmostEqual(result['sizing']['post_risk_sent'],.04)
        self.assertAlmostEqual(result['sizing']['post_risk_round_trip_commission'],.28)
        self.assertAlmostEqual(self.mt.sends[0]['volume'],.04)
        self.assertLessEqual(result['risk_sizing']['estimated_stop_loss_including_costs'],5.)

    def test_flexible_margin_estimate_failure_does_not_guess(self):
        self.cmd.update(demo_flexible=True)
        self.mt.order_calc_margin=lambda *args:None
        self.assertIn('estimate unavailable',self.send()['reason']);self.assertFalse(self.mt.sends)
    def test_filled_once_even_after_restart_and_duplicate_delivery(self):
        r=self.send();self.assertEqual(r['state'],'filled');self.assertEqual(self.mt.sends[0]['type_filling'],0);self.assertLess(self.mt.sends[0]['sl'],self.mt.tick.bid);self.assertGreater(self.mt.sends[0]['tp'],self.mt.tick.ask)
        self.ex.close();self.ex=BrokerExecutor(self.path);self.assertEqual(self.send()['state'],'filled');self.assertEqual(len(self.mt.sends),1)
    def test_timeout_never_retries_and_later_history_reconciles(self):
        self.mt.raise_send=True;self.assertEqual(self.send()['state'],'unknown');self.mt.raise_send=False;self.assertEqual(self.send()['state'],'unknown');self.assertEqual(len(self.mt.sends),1)
        self.mt.deals=[N(ticket=77,magic=self.cmd['magic'],symbol='XAUUSD',volume=.1,
                         type=0,entry=0,position_id=700,price=2000.10,time_msc=clock_ms(),comment='BT:'+self.cmd['id'][:20])];self.assertEqual(self.send()['state'],'filled');self.assertEqual(len(self.mt.sends),1)
    def test_broker_done_without_deal_ticket_is_not_falsely_booked_as_filled(self):
        self.mt.reply_deal=0
        first=self.send()
        self.assertEqual(first['retcode'],10009)
        self.assertEqual(first['state'],'placed')
        self.assertEqual(first['deal'],0)
        self.assertEqual(len(self.mt.sends),1)
        self.assertEqual(self.send()['state'],'placed')
        self.assertEqual(len(self.mt.sends),1)
        # A different broker comment can still be linked to the exact original
        # order number. Never synthesize fills or send the order twice.
        self.mt.deals=[N(ticket=78,order=55,position_id=900,magic=self.cmd['magic'],symbol='XAUUSD',
                         volume=.1,type=0,entry=0,comment='broker-stripped-comment',price=2000.10,time_msc=clock_ms())]
        result=self.send()
        self.assertEqual(result['state'],'filled')
        self.assertEqual(result['deal'],78)
        self.assertEqual(result['order'],55)
        self.assertEqual(result['position_id'],900)
        self.assertTrue(result['recovered_from_history'])
        self.assertEqual(len(self.mt.sends),1)
        self.ex.close();self.ex=BrokerExecutor(self.path)
        self.assertEqual(self.send()['state'],'filled')
        self.assertEqual(len(self.mt.sends),1)

    def test_broker_history_different_order_does_not_fill_an_unrelated_intent(self):
        self.mt.reply_deal=0
        self.assertEqual(self.send()['state'],'placed')
        self.mt.deals=[N(ticket=78,order=999,position_id=900,magic=self.cmd['magic'],symbol='XAUUSD',
                         volume=.1,type=0,entry=0,comment='another-order',price=2000.10,time_msc=clock_ms())]
        self.assertEqual(self.send()['state'],'placed')
        self.assertEqual(len(self.mt.sends),1)

    def test_conflicting_order_id_cannot_be_overridden_by_matching_comment(self):
        self.mt.reply_deal=0
        self.assertEqual(self.send()['state'],'placed')
        self.mt.deals=[N(ticket=88,order=999,position_id=900,magic=self.cmd['magic'],symbol='XAUUSD',
                         volume=.1,type=0,entry=0,comment='BT:'+self.cmd['id'][:20],price=2000.10,time_msc=clock_ms())]
        self.assertEqual(self.send()['state'],'placed')
        self.assertEqual(len(self.mt.sends),1)

    def test_partial_retcode_without_deal_ticket_waits_for_reconciliation(self):
        self.mt.code=10010;self.mt.reply_deal=0
        result=self.send()
        self.assertEqual(result['state'],'placed')
        self.assertEqual(result['retcode'],10010)
        self.assertEqual(len(self.mt.sends),1)
        self.assertEqual(self.send()['state'],'placed')

    def test_partial_close_reconciliation_retains_submitted_volume_after_crash(self):
        self.cmd.update(kind='close',position=1)
        self.mt.positions=[N(ticket=1,magic=self.cmd['magic'],symbol='XAUUSD',volume=.1,type=0)]
        self.mt.raise_send=True;self.assertEqual(self.send()['state'],'unknown')
        self.ex.db.execute('UPDATE sends SET result=NULL');self.ex.db.commit()
        self.ex.close();self.ex=BrokerExecutor(self.path)
        self.mt.deals=[N(ticket=77,magic=self.cmd['magic'],symbol='XAUUSD',volume=.04,
                         type=1,entry=1,position_id=700,price=2000.,time_msc=clock_ms(),comment='BT:'+self.cmd['id'][:20])]
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
        self.cmd['target']='live'
        self.assertEqual(self.send()['state'],'rejected')
        self.mt.account.trade_mode=2
        no_budget=self.send()
        self.assertEqual(no_budget['state'],'rejected')
        self.assertIn('no Core risk budget',no_budget['reason'])
        self.assertFalse(self.mt.sends)
        self.cmd.update(risk_percent=.25,commission_per_lot=7.,slippage_ticks=2.)
        self.assertEqual(self.send()['state'],'filled')
        self.assertAlmostEqual(self.mt.sends[0]['volume'],.1)
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
        self.cmd.update(kind='close',position=1)
        self.mt.positions=[N(ticket=1,magic=999,symbol='XAUUSD',volume=.05,type=0)]
        unrelated=self.send()
        self.assertEqual(unrelated['state'],'cancelled',unrelated)
        self.assertIn('no close order sent',unrelated['reason'])
        self.assertFalse(self.mt.sends)
        self.mt.positions.clear()
        absent=self.send()
        self.assertEqual(absent['state'],'cancelled',absent)
        self.assertIn('Verify exit from broker Deal history',absent['reason'])
        self.assertFalse(self.mt.sends)
        self.mt.positions=[N(ticket=1,magic=self.cmd['magic'],symbol='XAUUSD',volume=.05,type=0)]
        self.assertEqual(self.send()['state'],'filled')
        req=self.mt.sends[0]
        self.assertEqual(req['volume'],.05)
        self.assertEqual(req['position'],1)
        self.assertEqual(req['type'],1)
    def test_manual_profit_close_rechecks_profit_before_send_and_never_retries(self):
        self.cmd.update(kind='close',position=1,manual_close=True,profit_only=True,expected_position_volume=.05,expected_position_side='BUY')
        p=N(ticket=1,magic=self.cmd['magic'],symbol='XAUUSD',volume=.05,type=0,profit=3.)
        self.mt.positions=[p]
        calls=0
        def authorize(_):
            nonlocal calls
            calls+=1
            if calls==2:p.profit=-1.
            return True
        result=self.ex.execute(self.mt,self.cmd,authorize)
        self.assertEqual(result['state'],'cancelled',result)
        self.assertIn('no longer has positive',result['reason'])
        self.assertFalse(self.mt.sends)
        p.profit=3.
        self.assertEqual(self.send()['state'],'cancelled')
        self.assertFalse(self.mt.sends)
    def test_manual_close_rechecks_ticket_side_volume_and_account(self):
        for attribute,new_value in [('ticket',2),('magic',999),('volume',.02),('type',1)]:
            self.cmd.update(id='close-changed-'+attribute,kind='close',position=1,manual_close=True,profit_only=False,expected_position_volume=.05,expected_position_side='BUY')
            p=N(ticket=1,magic=self.cmd['magic'],symbol='XAUUSD',volume=.05,type=0,profit=4.)
            self.mt.positions=[p]
            calls=0
            def authorize(_,p=p,a=attribute,v=new_value):
                nonlocal calls
                calls+=1
                if calls==2:setattr(p,a,v)
                return True
            result=self.ex.execute(self.mt,self.cmd,authorize)
            self.assertEqual(result['state'],'cancelled',(attribute,result))
            self.assertFalse(self.mt.sends)
        self.cmd.update(id='close-changed-account',kind='close',position=1,manual_close=True,profit_only=False)
        self.mt.positions=[N(ticket=1,magic=self.cmd['magic'],symbol='XAUUSD',volume=.05,type=0,profit=4.)]
        self.mt.account_reads=0
        self.mt.on_account=lambda count:setattr(self.mt.account,'login',99) if count==6 else None
        result=self.send();self.assertEqual(result['state'],'cancelled',result)
        self.assertFalse(self.mt.sends)
        self.mt.on_account=None;self.mt.account.login=17
        self.cmd.update(id='close-approved-once',kind='close',position=1,manual_close=True,profit_only=True)
        self.mt.positions=[N(ticket=1,magic=self.cmd['magic'],symbol='XAUUSD',volume=.05,type=0,profit=4.)]
        result=self.send();self.assertEqual(result['state'],'filled',result)
        self.assertEqual(len(self.mt.sends),1)
        self.send();self.assertEqual(len(self.mt.sends),1)
        self.cmd['profit_only']=False
        self.assertEqual(self.send()['state'],'unknown')
        self.assertEqual(len(self.mt.sends),1)
    def test_filling_policy_and_broker_rejection(self):
        self.mt.info.filling_mode=2;self.mt.code=10019;self.assertEqual(self.send()['state'],'rejected');self.assertEqual(self.mt.sends[0]['type_filling'],1)
    def test_missing_journal_unknown_never_becomes_new_order(self):
        self.cmd['state']='unknown'
        self.assertEqual(self.send()['state'],'unknown')
        self.assertFalse(self.mt.sends)

    def test_every_non_new_core_state_without_durable_journal_never_resends(self):
        # Even a stale callback that claims authorisation cannot make a
        # second broker send for filled / partial / observed / terminal state.
        # This models lost local journal after a crash or Windows restore.
        for state in ('unknown','placed','partial','filled','observed',
                      'closed','cancelled','rejected','expired',''):
            self.cmd['id']='missing-journal-'+(state or 'empty')
            self.cmd['state']=state
            authorized=[]
            result=self.ex.execute(
                self.mt,self.cmd,lambda ident: authorized.append(ident) or True)
            self.assertEqual(result['state'],'unknown',(state,result))
            self.assertIn('never resend',result['reason'])
            self.assertFalse(authorized,(state,authorized))
            self.assertFalse(self.mt.sends,(state,self.mt.sends))
            self.assertIsNone(self.ex.db.execute(
                'SELECT id FROM sends WHERE id=?',(self.cmd['id'],)).fetchone())
        # A new queued Core work response can be accepted only once
        # Core's second authorization verifies its current dispatched
        # state; it gets a durable journal before order_send.
        self.cmd['id']='fresh-intent-after-stale'
        self.cmd['state']='queued'
        result=self.send()
        self.assertEqual(result['state'],'filled',result)
        self.assertEqual(len(self.mt.sends),1)
        self.assertEqual(self.send()['state'],'filled')
        self.assertEqual(len(self.mt.sends),1)
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
    def test_sniper_fast_path_rejects_unhonorable_stop_instead_of_widening(self):
        self.cmd['signal'].update(key='tick:geometry',stop_distance=.02,target_distance=.02)
        result=self.send()
        self.assertEqual(result['state'],'rejected')
        self.assertIn('SL was not widened',result['reason'])
        self.assertFalse(self.mt.sends)
    def test_sniper_fast_path_exact_five_pips_after_entry(self):
        self.cmd['signal'].update(key='tick:five-pips',stop_distance=1.,target_distance=.50)
        result=self.send()
        self.assertEqual(result['state'],'filled',result)
        self.assertEqual(result['pricing']['policy'],'sniper_send_geometry_v2_exact_tp')
        self.assertAlmostEqual(result['pricing']['target_distance'],.50,places=9)
        request=self.mt.sends[0]
        self.assertAlmostEqual(request['tp']-request['price'],.50,places=9)
    def test_sniper_ten_pips_are_one_gold_price_unit_even_if_broker_uses_0_1_points(self):
        self.mt.info.point=.10
        self.mt.info.trade_tick_size=.10
        self.mt.info.trade_stops_level=1
        self.cmd['signal'].update(key='tick:ten-pips',stop_distance=1.,target_distance=1.00)
        result=self.send()
        self.assertEqual(result['state'],'filled',result)
        self.assertAlmostEqual(self.mt.sends[0]['tp']-self.mt.sends[0]['price'],1.0,places=9)
    def test_sniper_target_not_a_multiple_of_broker_tick_is_never_rounded_or_sent(self):
        self.mt.info.point=.1
        self.mt.info.trade_tick_size=.1
        self.mt.info.trade_stops_level=1
        self.cmd['signal'].update(key='tick:nonrepresentable',stop_distance=1.,target_distance=.55)
        result=self.send()
        self.assertEqual(result['state'],'rejected')
        self.assertIn('not a multiple of broker tick size',result['reason'])
        self.assertIn('TP was NOT rounded or widened',result['reason'])
        self.assertFalse(self.mt.sends)

    def test_sniper_large_broker_stops_level_cannot_silently_extend_target_to_five_dollars(self):
        self.mt.info.trade_stops_level=500
        self.cmd['signal'].update(key='tick:broker-floor',stop_distance=6.,target_distance=.50)
        result=self.send()
        self.assertEqual(result['state'],'rejected')
        self.assertIn('TP was NOT widened',result['reason'])
        self.assertFalse(self.mt.sends)
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
