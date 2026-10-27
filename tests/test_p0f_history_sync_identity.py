"""P0-F: read-only broker history never crosses Demo/REAL or invents booked fees."""
from pathlib import Path
from types import SimpleNamespace as N
from unittest import TestCase, main
import sys
import tempfile
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'gateway'))
sys.path.insert(0,str(ROOT/'tests'))
from execution import BrokerExecutor,clock_ms
from test_broker_execution import Broker

class HistorySyncIdentityTests(TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.executor=BrokerExecutor(Path(self.tmp.name)/'journal.sqlite')
        self.mt=Broker()
        self.events=[]
        self.start=clock_ms()-10_000
        self.mt.deals=[N(ticket=808,position_id=123,magic=410601,symbol='XAUUSD',
            entry=0,type=0,time_msc=self.start+1000,volume=.1,price=2500.,
            profit=2.,commission=-.7,swap=-.25,fee=-.05,reason=1)]
    def tearDown(self):
        self.executor.close();self.tmp.cleanup()
    def api(self,path,payload=None):
        self.events.append((path,payload))
        return {'ok':True}
    def work(self,**overrides):
        identity=dict(login=17,server='Demo',currency='USD',trade_mode=0)
        identity.update(overrides)
        return dict(command=None,history_from_ms=self.start,account=identity,poll_ms=250)
    def test_matching_demo_syncs_only_exact_broker_booked_values(self):
        self.executor.cycle(self.mt,self.api,work=self.work())
        self.assertEqual(len(self.events),1)
        path,payload=self.events[0]
        self.assertEqual(path,'/v1/execution/sync')
        self.assertEqual(payload['trade_mode'],0)
        self.assertEqual(payload['deals'][0]['fee'],-.05)
        self.assertEqual(payload['deals'][0]['commission'],-.7)
        self.assertEqual(self.mt.sends,[])
    def test_live_account_cannot_inherit_demo_history_for_same_login(self):
        self.mt.account.trade_mode=2
        self.executor.cycle(self.mt,self.api,work=self.work())
        self.assertEqual(self.events,[])
    def test_account_mode_switch_during_history_read_discards_batch(self):
        def switch(n):
            if n==2:self.mt.account.trade_mode=2
        self.mt.on_account=switch
        self.executor.cycle(self.mt,self.api,work=self.work())
        self.assertEqual(self.events,[])
    def test_missing_or_nonfinite_costs_do_not_sync_as_zero(self):
        for fee in ('absent',float('nan'),float('inf')):
            with self.subTest(fee=fee):
                values=vars(self.mt.deals[0]).copy()
                if fee=='absent':values.pop('fee')
                else:values['fee']=fee
                self.mt.deals=[N(**values)]
                self.events.clear()
                self.executor.next_history=0
                self.executor.cycle(self.mt,self.api,work=self.work())
                self.assertEqual(self.events,[])
    def test_legacy_core_identity_without_mode_is_not_accepted(self):
        work=self.work()
        del work['account']['trade_mode']
        self.executor.cycle(self.mt,self.api,work=work)
        self.assertEqual(self.events,[])

if __name__=='__main__':main()
