"""Fault injection at the durable send boundary; fake brokers only."""
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace as N
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'gateway'))
from execution import BrokerExecutor, clock_ms
from test_broker_execution import Broker, command


class DispatchBoundaries(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.path = Path(self.tmp.name) / 'journal.sqlite'
        self.ex = BrokerExecutor(self.path)
        self.b = Broker()
        self.cmd = command()

    def tearDown(self):
        self.ex.close()
        self.tmp.cleanup()

    def send(self):
        return self.ex.execute(self.b, self.cmd, lambda _: True)

    def test_unreadable_prior_close_never_allows_a_second_close(self):
        self.b.positions = [N(ticket=41, magic=410601, symbol='XAUUSD', volume=.1, type=0)]
        self.cmd.update(kind='close', position=41)
        prior = dict(self.cmd, id='older-close')
        for saved in ('{', '{}', 'null', '[]', '{"state":null}', '{"state":"unexpected"}'):
            with self.subTest(saved=saved):
                self.ex.db.execute('DELETE FROM sends')
                self.ex.db.execute('INSERT INTO sends VALUES(?,?,?,?)',
                                   (prior['id'], json.dumps(prior), saved, clock_ms()))
                self.ex.db.commit()
                result = self.send()
                self.assertEqual(result['state'], 'cancelled', result)
                self.assertEqual(self.b.sends, [])

    def test_terminal_prior_close_does_not_block_a_new_intent(self):
        self.b.positions = [N(ticket=41, magic=410601, symbol='XAUUSD', volume=.1, type=0)]
        self.cmd.update(kind='close', position=41)
        for state in ('cancelled', 'rejected', 'filled'):
            with self.subTest(state=state):
                self.ex.db.execute('DELETE FROM sends')
                prior = dict(self.cmd, id='older-close')
                self.ex.db.execute('INSERT INTO sends VALUES(?,?,?,?)',
                                   (prior['id'], json.dumps(prior), json.dumps({'state':state}), clock_ms()))
                self.ex.db.commit()
                self.assertEqual(self.send()['state'], 'filled')
        self.assertEqual(len(self.b.sends), 3)

    def test_invalid_fill_ack_stays_unknown_across_restart_without_resending(self):
        cases = [dict(volume=0), dict(volume=-.1), dict(volume=.2),
                 dict(volume=.05), dict(price=0), dict(price=-1),
                 dict(order=-55), dict(deal=-77)]
        for index, changes in enumerate(cases):
            with self.subTest(changes=changes):
                self.cmd['id'] = f'invalid-ack-{index}'
                def bad_ack(request):
                    self.b.sends.append(request)
                    values = dict(retcode=10009, order=55, deal=77,
                                  volume=request['volume'], price=request['price'], comment='done')
                    values.update(changes)
                    return N(**values)
                self.b.order_send = bad_ack
                count = len(self.b.sends)
                result = self.send()
                self.assertEqual(result['state'], 'unknown', result)
                self.ex.close()
                self.ex = BrokerExecutor(self.path)
                self.assertEqual(self.send()['state'], 'unknown')
                self.assertEqual(len(self.b.sends), count+1)

    def test_invalid_ack_can_recover_from_exact_order_after_comment_is_removed(self):
        def bad_ack(request):
            self.b.sends.append(request)
            return N(retcode=10009, order=55, deal=77, volume=0, price=0, comment='done')
        self.b.order_send = bad_ack
        result = self.send()
        self.assertEqual(result['state'], 'unknown', result)
        self.b.deals = [N(ticket=77, order=55, magic=410601, symbol='XAUUSD',
                          volume=.1, type=0, entry=0, position_id=55,
                          comment='', price=2000.10, time_msc=clock_ms())]
        result = self.send()
        self.assertEqual(result['state'], 'filled', result)
        self.assertTrue(result['recovered_from_history'])
        self.assertEqual(len(self.b.sends), 1)

    def test_rejected_and_placed_zero_volume_ack_remain_valid(self):
        for code, state in ((10006, 'rejected'), (10008, 'placed')):
            self.cmd['id'] = str(code)
            def ack(request):
                self.b.sends.append(request)
                return N(retcode=code, order=55, deal=0, volume=0, price=0, comment='ack')
            self.b.order_send = ack
            self.assertEqual(self.send()['state'], state)

    def test_real_partial_fill_is_not_mistaken_for_full_fill(self):
        def ack(request):
            self.b.sends.append(request)
            return N(retcode=10010, order=55, deal=77, volume=.05, price=2000.10, comment='partial')
        self.b.order_send = ack
        self.assertEqual(self.send()['state'], 'partial')

    def test_automatic_entry_rechecks_account_after_last_quote_read(self):
        reads = []
        def tick(*_):
            reads.append(1)
            if len(reads) == 2:
                self.b.account.login = 99
            return self.b.tick
        self.b.symbol_info_tick = tick
        result = self.send()
        self.assertEqual(result['state'], 'cancelled', result)
        self.assertEqual(self.b.sends, [])

    def test_automatic_entry_rechecks_equity_after_last_quote_read(self):
        self.cmd.update(risk_percent=1., commission_per_lot=7., slippage_ticks=2.)
        reads = []
        def tick(*_):
            reads.append(1)
            if len(reads) == 2:
                self.b.account.equity = 1.
            return self.b.tick
        self.b.symbol_info_tick = tick
        result = self.send()
        self.assertEqual(result['state'], 'cancelled', result)
        self.assertEqual(self.b.sends, [])

    def test_new_position_during_entry_preflight_prevents_a_second_owner(self):
        for netting in (False, True):
            with self.subTest(netting=netting):
                self.cmd['id'] = str(netting)
                self.b.positions = []
                self.b.account.margin_mode = 0 if netting else 2
                self.b.on_check = lambda: self.b.positions.append(
                    N(ticket=42, magic=999 if netting else 410601, symbol='XAUUSD', volume=.1, type=0))
                result = self.send()
                self.assertEqual(result['state'], 'cancelled', result)
                self.assertEqual(self.b.sends, [])

    def test_other_brain_on_hedging_account_remains_independent(self):
        self.b.on_check = lambda: self.b.positions.append(
            N(ticket=42, magic=410602, symbol='XAUUSD', volume=.1, type=0))
        result = self.send()
        self.assertEqual(result['state'], 'filled', result)


if __name__ == '__main__':
    unittest.main()
