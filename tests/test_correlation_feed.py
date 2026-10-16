from datetime import datetime, timezone
from pathlib import Path
import math
import sys
from types import SimpleNamespace as N
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'gateway'))
from correlation_feed import aligned_returns,pair_statistics,collect_correlation,WINDOW
from fast_io import read_job

class Feed:
    TIMEFRAME_M5=5
    POSITION_TYPE_BUY=0
    def __init__(self,now):
        self.account=N(login=17,server='Demo',currency='USD');self.end=int(now.timestamp())//300*300-300
        self.rows=[dict(time=self.end-(240-i)*300,close=100.*math.exp(.002*math.sin(i*.3))) for i in range(241)]
        self.missing=set();self.switch=False
    def account_info(self):return self.account
    def symbol_info(self,symbol):return None if symbol in self.missing else N(visible=True)
    def copy_rates_from_pos(self,*args):
        if self.switch:self.account=N(login=18,server='Demo',currency='USD')
        return self.rows
    def positions_get(self):return [N(symbol='XAUUSD',magic=410614,volume=.1,type=0),N(symbol='XAUUSD',magic=410601,volume=.03,type=1),N(symbol='XAUUSD',magic=42,volume=10.,type=0)]

class CorrelationTests(unittest.TestCase):
    def test_perfect_inverse_and_constant_returns(self):
        a=[math.sin(i*.2) for i in range(100)]
        self.assertAlmostEqual(pair_statistics(a,a)['correlation'],1.)
        self.assertAlmostEqual(pair_statistics(a,[-x for x in a])['correlation'],-1.)
        self.assertIsNone(pair_statistics([1.]*100,a)['correlation'])
    def test_gaps_are_not_filled_or_bridged(self):
        rows=[dict(time=t,close=c) for t,c in [(60,100.),(120,101.),(240,105.),(300,106.)]]
        returns=aligned_returns(rows,[120,180,240,300],60)
        self.assertIsNone(returns[1]);self.assertIsNone(returns[2]);self.assertAlmostEqual(returns[3],math.log(106./105.))
    def test_minimum_and_pairwise_masks_use_exact_overlap(self):
        a=[float(i) for i in range(100)];b=[None]*41+[float(i) for i in range(59)]
        report=pair_statistics(a,b);self.assertEqual(report['samples'],59);self.assertIsNone(report['correlation']);self.assertEqual(sum(report['mask']),59)
        with self.assertRaises(ValueError):pair_statistics([1.],[1.,2.])
        with self.assertRaises(ValueError):pair_statistics([math.nan],[1.])
    def test_invalid_or_duplicate_prices_rejected(self):
        for rows in [[dict(time=60,close=0.)],[dict(time=61,close=1.)],[dict(time=60,close=1.)]*2]:
            with self.assertRaises(ValueError):aligned_returns(rows,[60],60)
    def fixture(self):
        now=datetime(2026,10,16,15,0,30,tzinfo=timezone.utc)
        job=dict(login=17,server='Demo',currency='USD',symbol='XAUUSD',symbols=['XAUUSD','EURUSD'],timeframe='M5',revision=1)
        return now,job,Feed(now)
    def test_collect_owned_exposure_and_completed_aligned_history(self):
        now,job,mt=self.fixture();r=collect_correlation(mt,job,now)
        self.assertEqual(r['series'][0]['available'],WINDOW);self.assertEqual(r['pairs'][0]['samples'],WINDOW)
        self.assertAlmostEqual(r['pairs'][0]['correlation'],1.)
        self.assertAlmostEqual(r['exposure'][0]['gross_lots'],.13);self.assertAlmostEqual(r['exposure'][0]['net_lots'],.07)
        self.assertEqual(r['exposure'][0]['positions'],2)
    def test_missing_symbol_and_future_candle_are_not_fabricated(self):
        now,job,mt=self.fixture();mt.missing.add('EURUSD')
        r=collect_correlation(mt,job,now);self.assertEqual(r['series'][1]['available'],0);self.assertIsNone(r['pairs'][0]['correlation'])
        mt.rows.append(dict(time=mt.end+600,close=101.));r=collect_correlation(mt,job,now)
        self.assertEqual(r['series'][0]['available'],0);self.assertIn('future',r['series'][0]['error'])
    def test_account_switch_discards_entire_report(self):
        now,job,mt=self.fixture();mt.switch=True
        with self.assertRaisesRegex(ValueError,'Account changed'):collect_correlation(mt,job,now)
    def test_read_worker_routes_correlation_without_order_adapter(self):
        now,job,mt=self.fixture()
        from unittest.mock import patch
        with patch('correlation_feed.datetime') as clock:
            clock.now.return_value=now
            r=read_job(mt,dict(kind='correlation',identity=(17,'Demo','XAUUSD','USD'),job=job),None)
        self.assertEqual(r['revision'],1);self.assertEqual(len(r['pairs']),1)

if __name__=='__main__':unittest.main()
