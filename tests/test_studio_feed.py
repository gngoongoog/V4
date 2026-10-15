import sys,unittest
from pathlib import Path
from types import SimpleNamespace as NS
from datetime import datetime,timezone
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'gateway'))
from studio_feed import collect_frame,TIMEFRAMES
class FeedTests(unittest.TestCase):
    def mock(self):
        a=NS(login=1,server='S',currency='USD');info=NS(point=.01,trade_tick_size=.01,trade_tick_value_loss=1.,trade_tick_value_profit=1.,volume_min=.01,volume_step=.01,volume_max=100.)
        row=lambda t:dict(time=t,open=100.,high=101.,low=99.,close=100.,tick_volume=100,spread=2,real_volume=0)
        mt5=NS(account_info=lambda:a,symbol_info=lambda _:info,copy_rates_from_pos=lambda *args:[row(1704067200),row(1706745600),row(1709251200)])
        for tf in TIMEFRAMES:setattr(mt5,'TIMEFRAME_'+tf,tf)
        return mt5,dict(login=1,server='S',symbol='X',currency='USD',timeframe='MN1')
    def test_all_frames_and_month_boundary(self):
        mt5,job=self.mock()
        for tf in TIMEFRAMES:
            f=collect_frame(mt5,dict(job,timeframe=tf),datetime(2024,3,2,tzinfo=timezone.utc));self.assertEqual(len(f['bars']),2);self.assertEqual(f['current_open'],1709251200);self.assertEqual(f['bars'][-1]['time'],1706745600)
    def test_account_switch_withholds_bars(self):
        mt5,job=self.mock();accounts=iter([NS(login=1,server='S',currency='USD'),NS(login=2,server='S',currency='USD')]);mt5.account_info=lambda:next(accounts)
        f=collect_frame(mt5,job,datetime(2024,3,2,tzinfo=timezone.utc));self.assertEqual(f['bars'],[]);self.assertIn('Account changed',f['error'])
    def test_missing_history_is_explicit(self):
        mt5,job=self.mock();mt5.copy_rates_from_pos=lambda *a:None;f=collect_frame(mt5,job,datetime(2024,3,2,tzinfo=timezone.utc));self.assertTrue(f['error']);self.assertEqual(f['bars'],[])
