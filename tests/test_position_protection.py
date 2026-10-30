"""Synthetic MT5-only tests. No broker connection or real order is used."""
import copy
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace as N

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'gateway'))
from execution import BrokerExecutor, clock_ms
from test_broker_execution import Broker


class ProtectiveBroker(Broker):
    TRADE_ACTION_SLTP = 6
    POSITION_TYPE_SELL = 1

    def __init__(self):
        super().__init__()
        self.info.trade_freeze_level = 3
        self.tick = N(bid=2001.0, ask=2001.1, time_msc=clock_ms())
        self.positions = [N(ticket=777, identifier=700, symbol='XAUUSD', magic=410601,
                            type=0, volume=.1, price_open=2000., sl=1999., tp=2005.)]
        self.apply_change = True

    def order_send(self, request):
        self.sends.append(copy.deepcopy(request))
        if self.raise_send:
            raise RuntimeError('connection lost')
        if self.apply_change and self.code == 10009:
            self.positions[0].sl = request['sl']
            self.positions[0].tp = request['tp']
        return N(retcode=self.code, comment='broker')


def intent():
    now = clock_ms()
    return dict(id='protect-1', kind='protect', state='queued', login=17, server='Demo', currency='USD',
                target='demo', symbol='XAUUSD', magic=410601, brain='gann', position=777,
                expected_position_identifier=700, expected_position_side='BUY', expected_position_volume=.1,
                expected_position_open=2000., expected_position_sl=1999., expected_position_tp=2005.,
                protect_stop=2000.5, protect_policy_id='policy-1', created_ms=now, expires_ms=now+30000,
                max_quote_age_ms=3000)


class PositionProtectionTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.path = Path(self.tmp.name) / 'journal.sqlite'
        self.ex = BrokerExecutor(self.path)
        self.mt = ProtectiveBroker()
        self.cmd = intent()

    def tearDown(self):
        self.ex.close()
        self.tmp.cleanup()

    def execute(self, authorize=lambda _: True):
        return self.ex.execute(self.mt, self.cmd, authorize)

    def test_buy_and_sell_tighten_sl_but_preserve_exact_tp(self):
        result = self.execute()
        self.assertEqual(result['state'], 'filled')
        self.assertEqual(self.mt.sends, [dict(action=6, symbol='XAUUSD', position=777, sl=2000.5, tp=2005.)])
        self.assertEqual(result['protection_observed']['identifier'], 700)
        self.cmd = intent()
        self.cmd.update(id='protect-sell', expected_position_side='SELL', expected_position_sl=2002.,
                        expected_position_tp=1995., protect_stop=1999.5)
        self.mt.positions[0].type=1
        self.mt.positions[0].sl=2002.
        self.mt.positions[0].tp=1995.
        self.mt.tick.bid=1998.9
        self.mt.tick.ask=1999.
        self.assertEqual(self.execute()['state'], 'filled')
        self.assertEqual(self.mt.sends[-1]['sl'], 1999.5)
        self.assertEqual(self.mt.sends[-1]['tp'], 1995.)

    def test_unknown_ack_is_never_resent_and_later_position_observation_resolves(self):
        self.mt.raise_send=True
        self.assertEqual(self.execute()['state'], 'unknown')
        self.assertEqual(len(self.mt.sends), 1)
        self.ex.close()
        self.ex=BrokerExecutor(self.path)
        self.assertEqual(self.execute()['state'], 'unknown')
        self.assertEqual(len(self.mt.sends), 1)
        self.mt.positions[0].sl=2000.6
        result=self.execute()
        self.assertEqual(result['state'], 'filled')
        self.assertEqual(result['protection_observed']['sl'], 2000.6)
        self.assertEqual(len(self.mt.sends), 1)

    def test_ack_is_not_proof_and_other_tp_does_not_resolve(self):
        self.mt.apply_change=False
        self.assertEqual(self.execute()['state'], 'placed')
        self.mt.positions[0].sl=2000.5
        self.mt.positions[0].tp=2006.
        self.assertEqual(self.execute()['state'], 'placed')
        self.assertEqual(len(self.mt.sends), 1)
        self.mt.positions[0].tp=2005.
        self.assertEqual(self.execute()['state'], 'filled')

    def test_unknown_modification_blocks_new_intent_for_same_position(self):
        self.mt.raise_send=True
        self.assertEqual(self.execute()['state'], 'unknown')
        self.cmd['id']='protect-2'
        self.assertEqual(self.execute()['state'], 'cancelled')
        self.assertEqual(len(self.mt.sends), 1)

    def test_immutable_identity_and_old_unjournaled_intent_never_resend(self):
        self.mt.apply_change=False
        self.assertEqual(self.execute()['state'], 'placed')
        self.cmd['protect_stop']=2000.6
        self.assertEqual(self.execute()['state'], 'unknown')
        self.assertEqual(len(self.mt.sends), 1)
        self.cmd=intent()
        self.cmd.update(id='old-id', state='unknown')
        self.assertEqual(self.execute()['state'], 'unknown')
        self.assertEqual(len(self.mt.sends), 1)

    def test_never_loosens_or_removes_the_existing_stop(self):
        for bad in (1998., 1999., 0., float('nan'), 2000.501):
            self.cmd['protect_stop']=bad
            self.assertEqual(self.execute()['state'], 'rejected', str(bad))
        self.assertEqual(self.mt.sends, [])

    def test_side_volume_owner_position_and_account_are_rechecked(self):
        for field, value in [('identifier',701),('symbol','EURUSD'),('magic',410602),('type',1),('volume',.2),
                             ('price_open',2000.01),('sl',1998.),('tp',2006.)]:
            p=self.mt.positions[0]
            original=getattr(p, field)
            setattr(p, field, value)
            self.assertEqual(self.execute()['state'], 'rejected', field)
            setattr(p, field, original)
        self.cmd['brain']='system'
        self.assertEqual(self.execute()['state'], 'rejected')
        self.cmd['brain']='gann'
        self.mt.account.trade_mode=2
        self.assertEqual(self.execute()['state'], 'rejected')
        self.assertEqual(self.mt.sends, [])

    def test_broker_distance_freeze_freshness_and_check_gate(self):
        self.mt.info.trade_stops_level=100
        self.assertEqual(self.execute()['state'], 'rejected')
        self.mt.info.trade_stops_level=5
        self.mt.info.trade_freeze_level=100
        self.assertEqual(self.execute()['state'], 'rejected')
        self.mt.info.trade_freeze_level=3
        self.mt.tick.time_msc=clock_ms()-4000
        self.assertEqual(self.execute()['state'], 'rejected')
        self.mt.tick.time_msc=clock_ms()
        self.mt.check_code=10016
        self.assertEqual(self.execute()['state'], 'rejected')
        self.assertEqual(self.mt.sends, [])

    def test_final_checks_after_journal_cancel_without_send(self):
        calls=[]
        def authorize(_):
            calls.append(True)
            if len(calls)==2:
                self.mt.positions[0].tp=2006.
            return True
        result=self.execute(authorize)
        self.assertEqual(result['state'], 'cancelled')
        self.assertEqual(self.mt.sends, [])
        self.mt.positions[0].tp=2005.
        self.assertEqual(self.execute()['state'], 'cancelled')
        self.assertEqual(self.mt.sends, [])

    def test_revoked_authorization_after_journal_is_terminal(self):
        calls=[]
        result=self.execute(lambda _: calls.append(1) or len(calls)<2)
        self.assertEqual(result['state'], 'cancelled')
        self.assertEqual(self.execute()['state'], 'cancelled')
        self.assertEqual(self.mt.sends, [])

    def test_protection_does_not_create_execution_quality_samples(self):
        self.assertEqual(self.execute()['state'], 'filled')
        count=self.ex.db.execute('SELECT COUNT(*) FROM execution_quality').fetchone()[0]
        self.assertEqual(count, 0)


if __name__ == '__main__':
    unittest.main()
