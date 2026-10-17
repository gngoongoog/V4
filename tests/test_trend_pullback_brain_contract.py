from pathlib import Path
import importlib.util
import unittest

ROOT = Path(__file__).resolve().parents[1]
DOMAIN = ROOT / "crates" / "domain" / "src" / "execution_legacy.rs"
PULLBACK = ROOT / "crates" / "domain" / "src" / "execution_pullback.rs"
GATEWAY = ROOT / "gateway" / "execution.py"


class TrendPullbackBrainContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.domain = DOMAIN.read_text(encoding="utf-8")
        cls.pullback = PULLBACK.read_text(encoding="utf-8")
        cls.gateway = GATEWAY.read_text(encoding="utf-8")

    def test_identity_is_appended_and_all_prior_magics_remain_stable(self):
        self.assertIn('pub const BRAINS: [(&str, &str); 16]', self.domain)
        liquidity = self.domain.index('(\"liquidity\", \"سحب السيولة — sweep + reclaim\")')
        pullback = self.domain.index('(\"trend_pullback\", \"Trend Pullback — رجوع منظم + تأكيد\")')
        self.assertLess(liquidity, pullback)
        self.assertIn('assert_eq!(magic("generated"), Some(410_613));', self.domain)
        self.assertIn('assert_eq!(magic("liquidity"), Some(410_614));', self.domain)
        self.assertIn('assert_eq!(magic("trend_pullback"), Some(410_615));', self.domain)

    def test_gateway_accepts_exact_appended_magic(self):
        self.assertIn('MAGIC_MIN, MAGIC_MAX = 410600, 410615', self.gateway)
        spec = importlib.util.spec_from_file_location("braintrade_gateway_execution_pullback", GATEWAY)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        self.assertEqual(module.MAGIC_MIN, 410600)
        self.assertEqual(module.MAGIC_MAX, 410615)

    def test_logic_requires_trend_pullback_confirmation_and_no_chase(self):
        for token in (
            'let fast = ema(&closes, 20);',
            'let slow = ema(&closes, 50);',
            'separation >= 0.15',
            'touched_fast',
            'body_ratio >= 0.45',
            'close_location >= 0.65',
            'distance_from_fast > 0.60',
            '(entry - last.close).abs() > 0.30 * atr',
            'stop_distance < 0.35 * atr',
            'stop_distance > 2.75 * atr',
        ):
            self.assertIn(token, self.pullback)
        self.assertNotIn('series[n]', self.pullback)
        self.assertIn('no future candle is consulted', self.pullback)

    def test_execution_uses_structural_stop_configured_rr_and_unique_signal_key(self):
        self.assertIn('"trend_pullback" => {', self.domain)
        self.assertIn('let q = pullback::assess(&series, t, atr);', self.domain)
        self.assertIn('s.stop_distance = q.stop_distance;', self.domain)
        self.assertIn('s.target_distance = b.reward_risk * s.stop_distance;', self.domain)
        self.assertIn('s.key = format!("pullback:{}:{}", b.timeframe, last.time);', self.domain)


if __name__ == "__main__":
    unittest.main()
