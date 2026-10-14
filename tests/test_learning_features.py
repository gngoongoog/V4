import hashlib
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
TRAINER = ROOT / "scripts" / "train_linear_baseline.py"
FIXTURE = ROOT / "tests" / "learning_feature_parity.json"

spec = importlib.util.spec_from_file_location("braintrade_train_linear_baseline", TRAINER)
trainer = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(trainer)


def fingerprint(values):
    payload = "".join(f"{name}={values[name]:.12f}\n" for name in sorted(values))
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


class LearningFeatureParityTests(unittest.TestCase):
    def test_python_flatten_matches_shared_fixture(self):
        fixture = json.loads(FIXTURE.read_text(encoding="utf-8"))
        values = trainer.flatten(fixture["features"])
        self.assertEqual(len(values), fixture["expected_feature_count"])
        self.assertEqual(fingerprint(values), fixture["expected_sha256"])

    def test_feature_contract_is_fixed_for_schema_v1(self):
        fixture = json.loads(FIXTURE.read_text(encoding="utf-8"))
        names = sorted(trainer.flatten(fixture["features"]))
        self.assertEqual(len(names), len(set(names)))
        self.assertIn("legacy_directional_score", names)
        self.assertIn("momentum.M1.rsi_centered", names)
        self.assertIn("gann.M15.eighth", names)
        self.assertIn("setup.pattern.breakout_retest", names)
        self.assertIn("session.vwap_present", names)


if __name__ == "__main__":
    unittest.main()
