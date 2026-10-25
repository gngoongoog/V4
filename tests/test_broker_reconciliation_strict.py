"""No live orders. Simulated broker history recovery guards for the P0-B gate."""
from __future__ import annotations
from pathlib import Path
from types import SimpleNamespace as N
import tempfile
import sys
import json
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'gateway'))
sys.path.insert(0, str(ROOT / 'tests'))
from execution import BrokerExecutor
from test_broker_execution import Broker, command


class BrokerReconciliationStrictTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.path = Path(self.tmp.name) / 'broker-journal.sqlite'
        self.exe = BrokerExecutor(self.path)
        self.broker = Broker()
        self.cmd = command()
        self.broker.raise_send = True
        first = self.exe.execute(self.broker, self.cmd, lambda _: True)
        self.assertEqual(first['state'], 'unknown', first)
        self.assertEqual(len(self.broker.sends), 1)
        self.broker.raise_send = False

    def tearDown(self):
        self.exe.close()
        self.tmp.cleanup()

    def deal(self, ticket, volume, *, order=0, **kw):
        return N(ticket=ticket, magic=self.cmd['magic'], symbol=self.cmd['symbol'],
                 comment='BT:' + self.cmd['id'][:20], volume=volume,
                 order=order, price=2000.0, position_id=kw.pop('position_id',700),
                 type=kw.pop('type',0), entry=kw.pop('entry',0),
                 time_msc=1000 + ticket, **kw)

    def result(self):
        out = self.exe.execute(self.broker, self.cmd, lambda _: True)
        self.assertEqual(len(self.broker.sends), 1, 'No broker resend during reconciliation')
        return out

    def test_mode_switch_blocks_recovery_even_with_same_login_tuple(self):
        self.broker.deals = [self.deal(899, .1)]
        self.broker.account.trade_mode = 2  # Same ID tuple, wrong Demo/REAL mode.
        result = self.result()
        self.assertEqual(result['state'], 'unknown', result)
        self.assertNotIn('recovered_from_history', result)

    def test_late_remaining_fill_upgrades_a_previous_partial(self):
        self.broker.deals = [self.deal(801, .04)]
        first = self.result()
        self.assertEqual(first['state'], 'partial', first)
        self.broker.deals = [self.deal(801, .04), self.deal(802, .06)]
        second = self.result()
        self.assertEqual(second['state'], 'filled', second)
        self.assertEqual(second['deals'], [801, 802])
        self.assertAlmostEqual(second['volume'], .1)
        self.assertEqual(self.result(), second)

    def test_excess_history_fill_is_never_booked_as_valid_exact_volume(self):
        self.broker.deals = [self.deal(803, .12)]
        result = self.result()
        self.assertEqual(result['state'], 'unknown', result)
        self.assertNotIn('recovered_from_history', result)
        self.assertIn('exceeds', result['reason'])

    def test_duplicate_broker_deal_tickets_cannot_be_counted_twice(self):
        self.broker.deals = [self.deal(804, .04), self.deal(804, .04)]
        result = self.result()
        self.assertEqual(result['state'], 'unknown', result)
        self.assertNotIn('recovered_from_history', result)
        self.assertIn('Duplicate', result['reason'])

    def test_invalid_history_volume_fails_closed(self):
        for amount in (float('nan'), -0.01, 0.0):
            with self.subTest(volume=amount):
                self.broker.deals = [self.deal(805, amount)]
                result = self.result()
                self.assertEqual(result['state'], 'unknown', result)
                self.assertNotIn('recovered_from_history', result)

    def test_recovered_partial_must_not_upgrade_on_wrong_order_ticket(self):
        self.broker.deals = [self.deal(806, .04)]
        first = self.result()
        self.assertEqual(first['state'], 'partial')
        self.broker.deals = [self.deal(806, .04), self.deal(807, .06, order=9999)]
        recorded = dict(first, order=42)
        self.exe.db.execute('UPDATE sends SET result=? WHERE id=?',
                            (json.dumps(recorded), self.cmd['id']))
        self.exe.db.commit()
        result = self.result()
        self.assertEqual(result['state'], 'partial', result)
        self.assertAlmostEqual(result['volume'], .04)


    def test_wrong_direction_history_cannot_resolve_unknown_intent(self):
        self.broker.deals = [self.deal(810, .1, type=self.broker.DEAL_TYPE_SELL)]
        result=self.result()
        self.assertEqual(result['state'],'unknown',result)
        self.assertIn('direction',result['reason'])
        self.broker.deals = [self.deal(810,.1)]
        self.assertEqual(self.result()['state'],'filled')
        self.assertEqual(len(self.broker.sends),1)

    def test_close_or_reversal_deal_cannot_fake_open_fill(self):
        for entry in (self.broker.DEAL_ENTRY_OUT, 2, self.broker.DEAL_ENTRY_OUT_BY):
            with self.subTest(entry=entry):
                self.broker.deals=[self.deal(811,.1,entry=entry)]
                r=self.result()
                self.assertEqual(r['state'],'unknown',r)
                self.assertIn('entry/exit',r['reason'])

    def test_missing_economic_deal_classification_blocks_learning_recovery(self):
        for field in ('type', 'entry', 'position_id'):
            with self.subTest(missing=field):
                self.broker.deals=[self.deal(812,.1)]
                delattr(self.broker.deals[0],field)
                r=self.result()
                self.assertEqual(r['state'],'unknown',r)
                self.assertIn('type/entry/position',r['reason'])
        self.broker.deals=[self.deal(812,.1,position_id=0)]
        r=self.result()
        self.assertEqual(r['state'],'unknown',r)
        self.assertIn('physical position',r['reason'])

    def test_mixed_physical_positions_not_summed_into_one_fill(self):
        self.broker.deals=[self.deal(813,.04),self.deal(814,.06,position_id=800)]
        r=self.result()
        self.assertEqual(r['state'],'unknown',r)
        self.assertIn('physical position',r['reason'])
        self.broker.deals=[self.deal(813,.04),self.deal(814,.06)]
        r=self.result()
        self.assertEqual(r['state'],'filled',r)
        self.assertEqual(r['position_id'],700)
        self.assertEqual(len(self.broker.sends),1)

    def test_restart_partial_then_incompatible_remaining_deal_stays_partial(self):
        self.broker.deals=[self.deal(815,.04)]
        self.assertEqual(self.result()['state'],'partial')
        self.exe.close()
        self.exe=BrokerExecutor(self.path)
        self.broker.deals=[self.deal(815,.04),self.deal(816,.06,entry=1)]
        r=self.result()
        self.assertEqual(r['state'],'partial',r)
        self.assertAlmostEqual(r['volume'],.04)
        self.broker.deals=[self.deal(815,.04),self.deal(816,.06)]
        r=self.result()
        self.assertEqual(r['state'],'filled',r)
        self.assertEqual(len(self.broker.sends),1)


    def test_close_recovery_needs_opposite_side_exit_not_opening_deal(self):
        # Separate durable close intent with a verified owned BUY position.
        # Tests use a fake MT5 broker, never a live account.
        with tempfile.TemporaryDirectory() as home:
            close_ex=BrokerExecutor(Path(home)/'close-journal.sqlite')
            broker=Broker()
            cmd=command()
            cmd.update(id='close-specific-history-proof',kind='close',position=881)
            broker.positions=[N(ticket=881,magic=cmd['magic'],symbol=cmd['symbol'],
                                volume=.1,type=broker.POSITION_TYPE_BUY)]
            broker.raise_send=True
            try:
                first=close_ex.execute(broker,cmd,lambda _:True)
                self.assertEqual(first['state'],'unknown',first)
                self.assertEqual(len(broker.sends),1)
                broker.raise_send=False
                tag='BT:'+cmd['id'][:20]
                def exit_deal(*, deal_type,entry):
                    return N(ticket=920,magic=cmd['magic'],symbol=cmd['symbol'],
                             comment=tag,volume=.1,order=0,position_id=881,
                             type=deal_type,entry=entry,price=2000.,time_msc=1200)
                # A BUY exit direction is wrong for closing a BUY position.
                # A SELL opening deal is not evidence of the CLOSE request.
                for typ,entry in ((broker.DEAL_TYPE_BUY,broker.DEAL_ENTRY_OUT),
                                  (broker.DEAL_TYPE_SELL,broker.DEAL_ENTRY_IN)):
                    broker.deals=[exit_deal(deal_type=typ,entry=entry)]
                    result=close_ex.execute(broker,cmd,lambda _:True)
                    self.assertEqual(result['state'],'unknown',result)
                    self.assertEqual(len(broker.sends),1)
                close_ex.close()
                close_ex=BrokerExecutor(Path(home)/'close-journal.sqlite')
                broker.deals=[exit_deal(deal_type=broker.DEAL_TYPE_SELL,
                                        entry=broker.DEAL_ENTRY_OUT)]
                result=close_ex.execute(broker,cmd,lambda _:True)
                self.assertEqual(result['state'],'filled',result)
                self.assertEqual(result['position_id'],881)
                self.assertTrue(result['recovered_from_history'])
                self.assertEqual(len(broker.sends),1)
            finally:
                close_ex.close()



if __name__ == '__main__':
    unittest.main()
