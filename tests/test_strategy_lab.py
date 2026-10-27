import sys
from pathlib import Path
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'gateway'))
from strategy_lab import study,load_verified_deals

def deal(i,mode='sniper',value=1):
    return {'deal_ticket':i+1,'closed_ms':1_700_000_000_000+i*86_400_000,'mode':mode,
            'symbol':'XAUUSD','login':321,'server':'DEMO','target':'demo',
            'profit':float(value),'commission':-0.7,'swap':-.1,'fee':-.2}

class StrategyLabTests(unittest.TestCase):
    def test_fee_swap_commission_all_count_once(self):
        r=study([deal(0,value=1),deal(1,value=-1)])
        self.assertEqual(r['modes']['sniper']['overall']['net'],-2)
        self.assertEqual(r['modes']['sniper']['overall']['deals'],2)
    def test_empty_evidence_never_generates_accuracy_or_fake_profit(self):
        r=study([])
        self.assertEqual(r['conclusion'],'UNVERIFIED')
        self.assertIsNone(r['modes']['expert']['overall']['win_rate'])
        self.assertIsNone(r['modes']['expert']['overall']['average_net_95pct_bootstrap'])
        self.assertFalse(r['automated_promotion_authorized'])
    def test_deduplicates_deals_and_rejects_missing_fees(self):
        with self.assertRaisesRegex(ValueError,'Duplicate'):
            load_verified_deals([deal(0),deal(0)])
        invalid=deal(2);invalid.pop('swap')
        with self.assertRaisesRegex(ValueError,'Missing'):
            load_verified_deals([invalid])
    def test_refuses_mixed_live_demo_or_account(self):
        bad=deal(0);bad['target']='live'
        with self.assertRaisesRegex(ValueError,'Demo'):load_verified_deals([bad])
        other=deal(1);other['login']=789
        with self.assertRaisesRegex(ValueError,'Mixing'):
            load_verified_deals([deal(0),other])
    def test_chronological_embargo_prevents_lookahead(self):
        data=[deal(i,('sniper','scalping','swing','expert')[i%4],2 if i%2 else -1) for i in range(200)]
        r=study(data)
        self.assertTrue(r['train_deals']<140)
        self.assertTrue(r['unseen_deals']<60)
        self.assertFalse(r['automated_promotion_authorized'])
    def test_bootstrap_reports_uncertainty_not_probability_of_success(self):
        data=[deal(i,value=2 if i%2 else -2) for i in range(120)]
        r=study(data)
        v=r['modes']['sniper']['overall']
        self.assertEqual(v['deals'],120)
        self.assertEqual(len(v['average_net_95pct_bootstrap']),2)
        self.assertTrue(v['average_net_95pct_bootstrap'][0]<v['average_net_95pct_bootstrap'][1])
        self.assertNotIn('intelligence',str(r))

if __name__=='__main__':unittest.main()
