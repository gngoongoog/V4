from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "analyze_feature_ablation.py"
FIXTURE = ROOT / "tests" / "learning_feature_parity.json"


def _module():
    spec = importlib.util.spec_from_file_location("braintrade_feature_ablation_test", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def _dataset(path: Path, rows: int = 600) -> Path:
    fixture = json.loads(FIXTURE.read_text(encoding="utf-8"))["features"]
    output = path / "dataset.jsonl"
    with output.open("w", encoding="utf-8") as handle:
        for index in range(rows):
            features = copy.deepcopy(fixture)
            direction = (-1, 0, 1)[index % 3]
            # Deliberately make legacy and trend perfectly correlated so the
            # redundancy diagnostic has a deterministic cross-group fixture.
            features["legacy_directional_score"] = direction
            features["trend"]["direction"] = direction
            features["trend"]["evidence"] = (index % 17) / 16.0
            features["regime"]["direction"] = -direction
            features["session"]["open_distance_atr"] = ((index % 41) - 20) / 4.0
            label = {
                "schema": "gold-m1-outcome-v1",
                "horizons": [
                    {"minutes": 5, "direction_0_2atr": direction},
                    {"minutes": 15, "direction_0_2atr": direction},
                    {"minutes": 30, "direction_0_2atr": direction},
                ],
            }
            row = {
                "context": "synthetic-ablation-context",
                "bar_time": 1_700_000_000 + index * 60,
                "snapshot_id": index + 1,
                "feature_schema": "gold-m1-feature-v1",
                "label_schema": "gold-m1-outcome-v1",
                "features": features,
                "label": label,
            }
            handle.write(json.dumps(row, separators=(",", ":")) + "\n")
    return output


class FeatureAblationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = _module()

    def test_feature_groups_cover_current_parity_fixture(self):
        fixture = json.loads(FIXTURE.read_text(encoding="utf-8"))
        names = sorted(self.module.trainer.flatten(fixture["features"]))
        self.assertEqual(len(names), fixture["expected_feature_count"])
        self.assertEqual(len(names), 87)
        grouped = self.module.grouped_features(names)
        self.assertEqual(grouped["ungrouped"], [])
        assigned = {name for group in self.module.GROUP_PREFIXES for name in grouped[group]}
        self.assertEqual(assigned, set(names))
        self.assertEqual(grouped["legacy"], ["legacy_directional_score"])
        self.assertTrue(all(name.startswith("momentum.") for name in grouped["momentum"]))
        self.assertEqual(self.module.feature_group("unknown.future_feature"), "ungrouped")

    def test_cross_group_correlation_ignores_constants_and_reports_duplicate_signal(self):
        rows = [
            {"x": {"legacy_directional_score": value, "trend.direction": value, "session.open_distance_atr": 1.0}}
            for value in (-1.0, -0.5, 0.0, 0.5, 1.0)
        ]
        result = self.module.high_cross_group_correlations(
            rows,
            ["legacy_directional_score", "session.open_distance_atr", "trend.direction"],
            threshold=0.99,
            max_pairs=10,
        )
        self.assertEqual(result["total_pairs"], 1)
        pair = result["reported_pairs"][0]
        self.assertEqual({pair["left_group"], pair["right_group"]}, {"legacy", "trend"})
        self.assertAlmostEqual(pair["correlation"], 1.0, places=12)
        self.assertTrue(result["constant_features_ignored"])

    def test_ablation_retrains_on_same_purged_split_and_is_deterministic(self):
        with tempfile.TemporaryDirectory() as directory:
            dataset = _dataset(Path(directory))
            arguments = dict(
                target_minutes=15,
                min_rows=100,
                epochs=2,
                groups=["legacy", "trend"],
                correlation_threshold=0.99,
                max_correlation_pairs=10,
            )
            first = self.module.analyze(dataset, **arguments)
            second = self.module.analyze(dataset, **arguments)
        self.assertEqual(first, second)
        self.assertEqual(first["schema"], "braintrade-feature-ablation-v1")
        self.assertEqual(first["status"], "research_only")
        self.assertFalse(first["eligible_for_promotion"])
        self.assertFalse(first["automatic_promotion"])
        self.assertFalse(first["prediction_enabled"])
        self.assertFalse(first["broker_effect"])
        self.assertFalse(first["method"]["random_split"])
        self.assertTrue(first["method"]["purged_chronological_split"])
        self.assertFalse(first["method"]["holdout_used_for_training"])
        self.assertEqual(first["dataset"]["purge_rows_each_side"], 30)
        self.assertEqual(first["full_model"]["feature_count"], 87)
        self.assertEqual([row["group"] for row in first["ablations"]], ["legacy", "trend"])
        self.assertTrue(all(row["remaining_feature_count"] < 87 for row in first["ablations"]))
        self.assertGreaterEqual(first["redundancy"]["total_pairs"], 1)
        self.assertEqual(len(first["analysis_id"]), 64)

    def test_ablation_rejects_unknown_or_duplicate_groups(self):
        with tempfile.TemporaryDirectory() as directory:
            dataset = _dataset(Path(directory))
            with self.assertRaisesRegex(ValueError, "unknown ablation"):
                self.module.analyze(dataset, min_rows=100, epochs=1, groups=["nope"])
            with self.assertRaisesRegex(ValueError, "duplicate ablation"):
                self.module.analyze(dataset, min_rows=100, epochs=1, groups=["trend", "trend"])


if __name__ == "__main__":
    unittest.main()
