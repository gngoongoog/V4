import unittest
import sys
import json
import sqlite3
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "gateway"))
from types import SimpleNamespace as N

from execution import _apply_sniper_fill_protection, clock_ms



class FakeExecutor:
    def __init__(self):
        self.db = sqlite3.connect(":memory:")
        self.db.execute("CREATE TABLE sends(id TEXT PRIMARY KEY, request TEXT NOT NULL, result TEXT, created_ms INTEGER NOT NULL)")
        self.db.commit()

    def _save(self, cmd, result):
        self.db.execute("UPDATE sends SET result=? WHERE id=?",
                        (json.dumps(result, allow_nan=False), cmd["id"]))
        self.db.commit()


class FakeMT5:
    TRADE_ACTION_SLTP = 6
    POSITION_TYPE_BUY = 0
    POSITION_TYPE_SELL = 1
    DEAL_TYPE_BUY = 0
    DEAL_TYPE_SELL = 1
    DEAL_ENTRY_IN = 0

    def __init__(self, *, modify_retcode=10009, deal_order=42, side=1):
        self.info = N(point=0.001, trade_tick_size=0.001,
                      trade_stops_level=0, trade_freeze_level=0)
        self.position = N(ticket=555, identifier=777, magic=1234, symbol="XAUUSD",
                          type=0 if side == 1 else 1, volume=0.10,
                          price_open=100.123, sl=99.120, tp=100.370)
        self.tick = N(bid=100.120, ask=100.121, time_msc=clock_ms())
        self.deal = N(ticket=99, order=deal_order, position_id=777, magic=1234,
                      symbol="XAUUSD", entry=0, type=0 if side == 1 else 1,
                      volume=0.10, price=100.123)
        self.modify_retcode = modify_retcode
        self.deals = [self.deal]
        self.sent = []
        self.executor = None
        self.protection_id = None

    def account_info(self):
        return N(login=1, server="Demo", currency="USD", trade_mode=0)

    def history_deals_get(self, start, end):
        return self.deals

    def positions_get(self, symbol=None):
        return [self.position]

    def symbol_info_tick(self, symbol):
        self.tick.time_msc = clock_ms()
        return self.tick

    def order_check(self, request):
        return N(retcode=0, comment="ok")

    def order_send(self, request):
        self.sent.append(request)
        row = self.executor.db.execute("SELECT request FROM sends WHERE id=?",
                                       (self.protection_id,)).fetchone()
        assert row is not None, "SLTP broker write must have a committed durable journal first"
        if self.modify_retcode in (10008, 10009):
            self.position.sl = request["sl"]
            self.position.tp = request["tp"]
        return N(retcode=self.modify_retcode, comment="test")


def command(side=1):
    return {
        "id": "test-open-123", "kind": "open", "created_ms": clock_ms() - 1000,
        "magic": 1234, "symbol": "XAUUSD",
        "login": 1, "server": "Demo", "currency": "USD", "target": "demo",
        "signal": {"key": "tick:123", "side": side,
                   "stop_distance": 1.0, "target_distance": 0.25,
                   "coordination": {"sniper_price_distance_policy": "sniper-price-distance-v1"}},
    }


class SniperFillProtectionTests(unittest.TestCase):
    def test_reanchors_and_verifies_actual_fill_position(self):
        mt5, executor, cmd = FakeMT5(), FakeExecutor(), command()
        mt5.executor, mt5.protection_id = executor, cmd["id"] + ":sniper-fill-protection"
        result = {"state": "filled", "deal": 99, "order": 42, "volume": 0.10}
        _apply_sniper_fill_protection(executor, mt5, cmd, mt5.info, N(), result)
        self.assertEqual(result["sniper_fill_protection"]["state"], "verified")
        self.assertAlmostEqual(mt5.position.sl, 99.123)
        self.assertAlmostEqual(mt5.position.tp, 100.373)
        self.assertEqual(len(mt5.sent), 1)
        row = executor.db.execute("SELECT request,result FROM sends WHERE id=?",
                                  (mt5.protection_id,)).fetchone()
        self.assertIsNotNone(row)
        self.assertEqual(json.loads(row[1])["state"], "filled")

    def test_repeated_reconciliation_observes_journaled_protection_without_second_send(self):
        mt5, executor, cmd = FakeMT5(), FakeExecutor(), command()
        mt5.executor, mt5.protection_id = executor, cmd["id"] + ":sniper-fill-protection"
        result = {"state": "filled", "deal": 99, "order": 42, "volume": 0.10}
        _apply_sniper_fill_protection(executor, mt5, cmd, mt5.info, N(), result)
        # Simulate a crash after the broker applied SLTP but before result save.
        executor.db.execute("UPDATE sends SET result=NULL WHERE id=?", (mt5.protection_id,))
        executor.db.commit()
        _apply_sniper_fill_protection(executor, mt5, cmd, mt5.info, N(), result)
        self.assertEqual(len(mt5.sent), 1)
        self.assertEqual(result["sniper_fill_protection"]["state"], "verified")

    def test_weighted_average_of_multiple_order_deals_matches_position(self):
        mt5, executor, cmd = FakeMT5(), FakeExecutor(), command()
        mt5.executor, mt5.protection_id = executor, cmd["id"] + ":sniper-fill-protection"
        mt5.deals.append(N(ticket=100, order=42, position_id=777, magic=1234,
                           symbol="XAUUSD", entry=0, type=0,
                           volume=0.10, price=100.133))
        mt5.position.volume = 0.20
        mt5.position.price_open = 100.128
        result = {"state": "filled", "deal": 99, "order": 42, "volume": 0.20}
        _apply_sniper_fill_protection(executor, mt5, cmd, mt5.info, N(), result)
        self.assertAlmostEqual(result["sniper_fill_protection"]["fill_price"], 100.128)
        self.assertAlmostEqual(mt5.position.sl, 99.128)
        self.assertAlmostEqual(mt5.position.tp, 100.378)

    def test_sell_direction_is_reversed(self):
        mt5, executor, cmd = FakeMT5(side=-1), FakeExecutor(), command(-1)
        mt5.executor, mt5.protection_id = executor, cmd["id"] + ":sniper-fill-protection"
        mt5.position.price_open = 100.123
        mt5.tick = N(bid=100.125, ask=100.126, time_msc=clock_ms())
        result = {"state": "filled", "deal": 99, "order": 42, "volume": 0.10}
        _apply_sniper_fill_protection(executor, mt5, cmd, mt5.info, N(), result)
        self.assertAlmostEqual(mt5.position.sl, 101.123)
        self.assertAlmostEqual(mt5.position.tp, 99.873)

    def test_wrong_order_link_is_rejected_without_modification(self):
        mt5, executor, cmd = FakeMT5(deal_order=999), FakeExecutor(), command()
        mt5.executor, mt5.protection_id = executor, cmd["id"] + ":sniper-fill-protection"
        result = {"state": "filled", "deal": 99, "order": 42, "volume": 0.10}
        with self.assertRaisesRegex(ValueError, "not uniquely visible"):
            _apply_sniper_fill_protection(executor, mt5, cmd, mt5.info, N(), result)
        self.assertEqual(mt5.sent, [])

    def test_broker_modification_rejection_is_not_reported_as_success(self):
        mt5, executor, cmd = FakeMT5(modify_retcode=10016), FakeExecutor(), command()
        mt5.executor, mt5.protection_id = executor, cmd["id"] + ":sniper-fill-protection"
        result = {"state": "filled", "deal": 99, "order": 42, "volume": 0.10}
        with self.assertRaisesRegex(ValueError, "broker rejected post-fill"):
            _apply_sniper_fill_protection(executor, mt5, cmd, mt5.info, N(), result)
        self.assertEqual(mt5.position.sl, 99.120)
        self.assertEqual(mt5.position.tp, 100.370)
        row = executor.db.execute("SELECT result FROM sends WHERE id=?",
                                  (mt5.protection_id,)).fetchone()
        self.assertEqual(json.loads(row[0])["state"], "rejected")
        with self.assertRaisesRegex(ValueError, "prior modification journal is unresolved"):
            _apply_sniper_fill_protection(executor, mt5, cmd, mt5.info, N(), result)
        self.assertEqual(len(mt5.sent), 1, "Uncertain SLTP modification must never be blindly resent")

    def test_partial_or_non_sniper_result_is_not_modified(self):
        mt5 = FakeMT5()
        result = {"state": "partial", "deal": 99, "order": 42}
        _apply_sniper_fill_protection(FakeExecutor(), mt5, command(), mt5.info, N(), result)
        self.assertEqual(mt5.sent, [])

    def test_terminal_partial_fill_is_reanchored_only_after_remainder_is_proven_gone(self):
        mt5, executor, cmd = FakeMT5(), FakeExecutor(), command()
        mt5.executor, mt5.protection_id = executor, cmd["id"] + ":sniper-fill-protection"
        result = {"state": "partial", "deal": 99, "order": 42, "volume": 0.10,
                  "terminal_order": {"confirmed": True, "state": "canceled"}}
        _apply_sniper_fill_protection(executor, mt5, cmd, mt5.info, N(), result)
        self.assertEqual(result["sniper_fill_protection"]["state"], "verified")
        self.assertEqual(len(mt5.sent), 1)

    def test_legacy_tick_signal_without_price_distance_marker_is_unchanged(self):
        mt5, executor = FakeMT5(), FakeExecutor()
        cmd = command()
        mt5.executor, mt5.protection_id = executor, cmd["id"] + ":sniper-fill-protection"
        cmd["signal"].pop("coordination")
        result = {"state": "filled", "deal": 99, "order": 42, "volume": 0.10}
        _apply_sniper_fill_protection(executor, mt5, cmd, mt5.info, N(), result)
        self.assertEqual(mt5.sent, [])
        self.assertNotIn("sniper_fill_protection", result)


if __name__ == "__main__":
    unittest.main()
