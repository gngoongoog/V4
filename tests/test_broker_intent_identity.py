"""V25 P0-C: identical journal ID must not reinterpret a changed economic intent."""
from __future__ import annotations
import copy
from pathlib import Path
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'gateway'))
sys.path.insert(0, str(ROOT/'tests'))
from execution import BrokerExecutor
from test_broker_execution import Broker,command


class BrokerIntentIdentityTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.executor=BrokerExecutor(Path(self.tmp.name)/'journal.sqlite')
        self.broker=Broker()
        self.original=command()
        self.broker.raise_send=True
        first=self.executor.execute(self.broker,self.original,lambda _:True)
        self.assertEqual(first['state'],'unknown',first)
        self.assertEqual(len(self.broker.sends),1)
        self.broker.raise_send=False

    def tearDown(self):
        self.executor.close()
        self.tmp.cleanup()

    def test_all_mutated_economic_intents_rejected_by_durable_id(self):
        variations={
            'target':'live',
            'lots':0.2,
            'signal':dict(self.original['signal'],side=-1),
            'revision':42,
            'expires_ms':self.original['expires_ms']+5000,
            'commission_per_lot':7.0,
            'deviation_points':100,
            'risk_percent':2.0,
            'demo_flexible':True,
            'daily_breaker_enabled':False,
        }
        for key,value in variations.items():
            with self.subTest(key=key):
                altered=copy.deepcopy(self.original)
                altered[key]=value
                result=self.executor.execute(self.broker,altered,lambda _:True)
                self.assertEqual(result['state'],'unknown',result)
                self.assertIn('Intent identity differs',result['reason'])
                self.assertEqual(len(self.broker.sends),1)

    def test_poll_state_and_latency_metadata_may_change_without_violating_identity(self):
        replay=copy.deepcopy(self.original)
        replay['state']='unknown'
        replay['_core_pulse_us']=512
        result=self.executor.execute(self.broker,replay,lambda _:True)
        self.assertEqual(result['state'],'unknown')
        self.assertNotIn('Intent identity differs',result['reason'])
        self.assertEqual(len(self.broker.sends),1)


if __name__=='__main__':
    unittest.main()
