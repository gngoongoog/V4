from pathlib import Path
import importlib.util
import unittest

ROOT = Path(__file__).resolve().parents[1]
DOMAIN = ROOT / "crates" / "domain" / "src" / "execution_legacy.rs"
LIQUIDITY = ROOT / "crates" / "domain" / "src" / "execution_liquidity.rs"
GATEWAY = ROOT / "gateway" / "execution.py"


class LiquidityBrainContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.domain = DOMAIN.read_text(encoding="utf-8")
        cls.liquidity = LIQUIDITY.read_text(encoding="utf-8")
        cls.gateway = GATEWAY.read_text(encoding="utf-8")

    def test_identity_is_appended_without_remapping_generated(self):
        self.assertIn('pub const BRAINS: [(&str, &str); 15]', self.domain)
        generated = self.domain.index('(\"generated\", \"استراتيجية مولّدة مختارة\")')
        liquidity = self.domain.index('(\"liquidity\", \"سحب السيولة — sweep + reclaim\")')
        self.assertLess(generated, liquidity)
        self.assertIn('assert_eq!(magic("generated"), Some(410_613));', self.domain)
        self.assertIn('assert_eq!(magic("liquidity"), Some(410_614));', self.domain)

    def test_gateway_accepts_exact_new_magic_ceiling(self):
        self.assertIn('MAGIC_MIN, MAGIC_MAX = 410600, 410614', self.gateway)
        spec = importlib.util.spec_from_file_location("braintrade_gateway_execution", GATEWAY)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        self.assertEqual(module.MAGIC_MIN, 410600)
        self.assertEqual(module.MAGIC_MAX, 410614)

    def test_liquidity_logic_is_causal_and_does_not_claim_hidden_orders(self):
        self.assertIn('let prior = &series[n - 21..n - 1];', self.liquidity)
        self.assertIn('out.activity_ratio >= 1.10', self.liquidity)
        self.assertIn('wick >= 0.35 * range', self.liquidity)
        self.assertIn('out.evidence < 0.62', self.liquidity)
        self.assertIn('(entry - last.close).abs() > 0.35 * atr', self.liquidity)
        self.assertIn('لا ادعاء رؤية أوامر وقف مخفية', self.liquidity)

    def test_execution_engine_uses_structural_stop_and_configured_reward(self):
        self.assertIn('"liquidity" => {', self.domain)
        self.assertIn('let q = liquidity::assess(&series, t, atr);', self.domain)
        self.assertIn('s.stop_distance = q.stop_distance;', self.domain)
        self.assertIn('s.target_distance = b.reward_risk * s.stop_distance;', self.domain)
        self.assertIn('s.key = format!("sweep:{}:{}", b.timeframe, last.time);', self.domain)


if __name__ == "__main__":
    unittest.main()
