import unittest
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "gateway"))
from types import SimpleNamespace as N

from execution import _apply_sniper_fill_protection, clock_ms


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
                          price_open=100.123, sl=99.123, tp=100.373)
        self.tick = N(bid=100.120, ask=100.121, time_msc=clock_ms())
        self.deal = N(ticket=99, order=deal_order, position_id=777, magic=1234,
                      symbol="XAUUSD", entry=0, type=0 if side == 1 else 1,
                      volume=0.10, price=100.123)
        self.modify_retcode = modify_retcode
        self.sent = []

    def history_deals_get(self, start, end):
        return [self.deal]

    def positions_get(self, symbol=None):
        return [self.position]

    def symbol_info_tick(self, symbol):
        self.tick.time_msc = clock_ms()
        return self.tick

    def order_check(self, request):
        return N(retcode=0, comment="ok")

    def order_send(self, request):
        self.sent.append(request)
        if self.modify_retcode in (10008, 10009):
            self.position.sl = request["sl"]
            self.position.tp = request["tp"]
        return N(retcode=self.modify_retcode, comment="test")


def command(side=1):
    return {
        "kind": "open", "created_ms": clock_ms() - 1000,
        "magic": 1234, "symbol": "XAUUSD",
        "signal": {"key": "tick:123", "side": side,
                   "stop_distance": 1.0, "target_distance": 0.25,
                   "coordination": {"sniper_price_distance_policy": "sniper-price-distance-v1"}},
    }


class SniperFillProtectionTests(unittest.TestCase):
    def test_reanchors_and_verifies_actual_fill_position(self):
        mt5 = FakeMT5()
        result = {"state": "filled", "deal": 99, "order": 42}
        _apply_sniper_fill_protection(mt5, command(), mt5.info, N(), result)
        self.assertEqual(result["sniper_fill_protection"]["state"], "verified")
        self.assertAlmostEqual(mt5.position.sl, 99.123)
        self.assertAlmostEqual(mt5.position.tp, 100.373)
        self.assertEqual(len(mt5.sent), 1)

    def test_sell_direction_is_reversed(self):
        mt5 = FakeMT5(side=-1)
        mt5.position.price_open = 100.123
        mt5.tick = N(bid=100.125, ask=100.126, time_msc=clock_ms())
        result = {"state": "filled", "deal": 99, "order": 42}
        _apply_sniper_fill_protection(mt5, command(-1), mt5.info, N(), result)
        self.assertAlmostEqual(mt5.position.sl, 101.123)
        self.assertAlmostEqual(mt5.position.tp, 99.873)

    def test_wrong_order_link_is_rejected_without_modification(self):
        mt5 = FakeMT5(deal_order=999)
        result = {"state": "filled", "deal": 99, "order": 42}
        with self.assertRaisesRegex(ValueError, "identity/direction"):
            _apply_sniper_fill_protection(mt5, command(), mt5.info, N(), result)
        self.assertEqual(mt5.sent, [])

    def test_broker_modification_rejection_is_not_reported_as_success(self):
        mt5 = FakeMT5(modify_retcode=10016)
        result = {"state": "filled", "deal": 99, "order": 42}
        with self.assertRaisesRegex(ValueError, "broker rejected post-fill"):
            _apply_sniper_fill_protection(mt5, command(), mt5.info, N(), result)
        self.assertEqual(mt5.position.sl, 99.123)
        self.assertEqual(mt5.position.tp, 100.373)

    def test_partial_or_non_sniper_result_is_not_modified(self):
        mt5 = FakeMT5()
        result = {"state": "partial", "deal": 99, "order": 42}
        _apply_sniper_fill_protection(mt5, command(), mt5.info, N(), result)
        self.assertEqual(mt5.sent, [])

    def test_legacy_tick_signal_without_price_distance_marker_is_unchanged(self):
        mt5 = FakeMT5()
        cmd = command()
        cmd["signal"].pop("coordination")
        result = {"state": "filled", "deal": 99, "order": 42}
        _apply_sniper_fill_protection(mt5, cmd, mt5.info, N(), result)
        self.assertEqual(mt5.sent, [])
        self.assertNotIn("sniper_fill_protection", result)


if __name__ == "__main__":
    unittest.main()
