import unittest
from types import SimpleNamespace as N

from sniper_price_geometry import sniper_levels_from_fill


class SniperPriceGeometryTests(unittest.TestCase):
    def setUp(self):
        self.info = N(point=0.001, trade_tick_size=0.001,
                      trade_stops_level=0, trade_freeze_level=0)

    def test_buy_uses_confirmed_fill_and_price_distances(self):
        got = sniper_levels_from_fill(self.info, 1, 4098.123, 1.0, 0.25)
        self.assertAlmostEqual(got["sl"], 4097.123)
        self.assertAlmostEqual(got["tp"], 4098.373)

    def test_sell_reverses_levels(self):
        got = sniper_levels_from_fill(self.info, -1, 4098.123, 1.0, 0.25)
        self.assertAlmostEqual(got["sl"], 4099.123)
        self.assertAlmostEqual(got["tp"], 4097.873)

    def test_refuses_distance_not_on_tick_grid(self):
        with self.assertRaisesRegex(ValueError, "cannot be represented exactly"):
            sniper_levels_from_fill(self.info, 1, 4098.123, 1.0, 0.2505)

    def test_refuses_fill_off_tick_grid(self):
        with self.assertRaisesRegex(ValueError, "fill price is outside"):
            sniper_levels_from_fill(self.info, 1, 4098.1234, 1.0, 0.25)

    def test_refuses_broker_minimum_without_widening(self):
        self.info.trade_stops_level = 300
        with self.assertRaisesRegex(ValueError, "not widened"):
            sniper_levels_from_fill(self.info, 1, 4098.123, 1.0, 0.25)

    def test_rejects_invalid_inputs(self):
        for side, fill, stop, target in ((0, 100, 1, 1), (1, 0, 1, 1),
                                         (1, 100, -1, 1), (1, 100, 1, float("nan"))):
            with self.subTest(side=side, fill=fill, stop=stop, target=target):
                with self.assertRaises(ValueError):
                    sniper_levels_from_fill(self.info, side, fill, stop, target)


if __name__ == "__main__":
    unittest.main()
