import importlib.util
from pathlib import Path
from datetime import datetime, timezone
from types import SimpleNamespace as NS
import sys
import unittest

ROOT=Path(__file__).parents[1]
sys.path.insert(0,str(ROOT/"gateway"))
spec=importlib.util.spec_from_file_location("gateway",ROOT/"gateway"/"mt5_gateway.py")
gateway=importlib.util.module_from_spec(spec)
spec.loader.exec_module(gateway)

class FakeMT5:
    ACCOUNT_TRADE_MODE_DEMO=0
    POSITION_TYPE_BUY=0
    DEAL_TYPE_BUY=0
    DEAL_TYPE_SELL=1
    def __init__(self):
        self.account=NS(login=123,server="Test",currency="USD",trade_mode=0,balance=1000.0,equity=1010.0)
        self.positions=[];self.orders=[];self.deals=[]
    def terminal_info(self):return NS(connected=True)
    def account_info(self):return self.account
    def positions_get(self):return self.positions
    def orders_get(self):return self.orders
    def history_deals_get(self,*args):return self.deals
    def symbol_info(self,*args):return NS(visible=True,volume_min=0.01,volume_step=0.01,trade_tick_size=0.01)
    def symbol_info_tick(self,*args):return NS(bid=2000.0,ask=2000.2,time_msc=100)

class Tests(unittest.TestCase):
    def history_fixture(self):
        start=int(datetime(2024,10,1,tzinfo=timezone.utc).timestamp())
        job=dict(id='fixture',**{'from':start,'to':start+86400},login=123,server='Test',currency='USD',symbol='X',point=.01)
        m=FakeMT5();m.TIMEFRAME_M1=1;m.symbol_info=lambda symbol:NS(point=.01)
        rows=[dict(time=start,open=2000.,high=2001.,low=1999.,close=2000.5,tick_volume=10,spread=20)]
        return m,job,rows
    def test_history_utc_end_is_exclusive(self):
        m,job,rows=self.history_fixture();calls=[]
        def read(symbol,tf,start,end):calls.append((start,end));return rows
        m.copy_rates_range=read
        result=gateway.collect_history_chunk(m,job)
        self.assertEqual(result['error'],'');self.assertEqual(result['bars'][0]['spread_points'],20)
        self.assertEqual(calls[0][0].tzinfo,timezone.utc)
        self.assertEqual(calls[0][1].timestamp(),job['to']-1)
    def test_history_account_switch_withholds_all_bars(self):
        m,job,rows=self.history_fixture()
        def read(*args):m.account=NS(login=456,server='Other',currency='USD');return rows
        m.copy_rates_range=read
        result=gateway.collect_history_chunk(m,job)
        self.assertTrue(result['error']);self.assertEqual(result['bars'],[])
    def test_history_missing_and_empty_are_distinct(self):
        m,job,rows=self.history_fixture();m.copy_rates_range=lambda *args:None
        self.assertTrue(gateway.collect_history_chunk(m,job)['error'])
        m.copy_rates_range=lambda *args:[]
        result=gateway.collect_history_chunk(m,job);self.assertEqual(result['error'],'');self.assertEqual(result['bars'],[])
    def test_history_duplicate_or_unfinished_is_rejected(self):
        m,job,rows=self.history_fixture();m.copy_rates_range=lambda *args:rows+rows
        self.assertTrue(gateway.collect_history_chunk(m,job)['error'])
        rows[0]['time']=job['to'];m.copy_rates_range=lambda *args:rows
        self.assertTrue(gateway.collect_history_chunk(m,job)['error'])
    def test_closed_bar_reader(self):
        now=datetime(2026,10,11,12,0,20,tzinfo=timezone.utc);calls=[]
        class Bars:
            TIMEFRAME_M1=1;TIMEFRAME_M5=5;TIMEFRAME_M15=15
            def copy_rates_from_pos(self,symbol,tf,pos,count):
                calls.append((tf,pos,count));boundary=int(now.timestamp())//(tf*60)*(tf*60)
                return [dict(time=boundary-(180-i)*tf*60,open=2000.,high=2001.,low=1999.,close=2000.5,tick_volume=100) for i in range(180)]
        d=gateway.collect_market(Bars(),'X',now)
        self.assertEqual(d['error'],'');self.assertEqual(len(d['m1']),180)
        self.assertTrue(all(p==1 and n==180 for _,p,n in calls))
    def test_market_cache_changes_with_account(self):
        from unittest.mock import patch
        cache=gateway.MarketCache();now=datetime.now(timezone.utc)
        with patch.object(gateway,'collect_market',return_value={}) as r:
            cache.read(None,(1,'A','X'),now);cache.read(None,(1,'A','X'),now);self.assertEqual(r.call_count,1)
            cache.read(None,(2,'A','X'),now);self.assertEqual(r.call_count,2)
    def test_missing_history_no_fabrication(self):
        d=gateway.collect_market(FakeMT5(),'X',datetime.now(timezone.utc))
        self.assertTrue(d['error']);self.assertEqual(d['m1'],[])
    def test_baghdad_midnight(self):
        start,end,day=gateway.day_range(datetime(2026,10,10,22,0,tzinfo=timezone.utc))
        self.assertEqual(day,"2026-10-11")
        self.assertEqual(start,datetime(2026,10,10,21,0,tzinfo=timezone.utc))
    def test_cashflows_excluded_and_fees_included(self):
        m=FakeMT5();m.deals=[NS(type=0,profit=10,commission=-2,swap=-1,fee=-0.5),NS(type=2,profit=500,commission=0,swap=0,fee=0)]
        s=gateway.collect(m,"XAUUSD");self.assertEqual(s["today_net"],6.5);self.assertEqual(s["today_deals"],1)
    def test_no_history_is_unknown_not_zero(self):
        m=FakeMT5();m.deals=None
        s=gateway.collect(m,"XAUUSD");self.assertIsNone(s["today_net"]);self.assertTrue(s["connected"])
    def test_failed_positions_is_not_empty_positions(self):
        m=FakeMT5();m.positions=None
        self.assertFalse(gateway.collect(m,"XAUUSD")["connected"])
    def test_account_switch_rejects_snapshot(self):
        m=FakeMT5();account=m.account;calls=[]
        def switch():
            calls.append(1)
            return account if len(calls)==1 else NS(login=456,server="Other")
        m.account_info=switch
        self.assertFalse(gateway.collect(m,"XAUUSD")["connected"])

if __name__=="__main__":unittest.main()
