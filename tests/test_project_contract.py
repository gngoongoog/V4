from __future__ import annotations

import json
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]


def tracked_files() -> list[Path]:
    try:
        output = subprocess.check_output(
            ["git", "ls-files", "-z"], cwd=ROOT, stderr=subprocess.DEVNULL
        )
        return [ROOT / item.decode("utf-8") for item in output.split(b"\0") if item]
    except (OSError, subprocess.CalledProcessError):
        return [path for path in ROOT.rglob("*") if path.is_file()]


class ProjectContractTests(unittest.TestCase):
    def test_tracked_tree_has_no_runtime_state_or_transport_artifacts(self):
        forbidden_top_level = {
            "target",
            "release",
            "runtime",
            "test-output",
            ".package-cache",
            ".build-venv",
            ".ci",
            ".venv",
            "venv",
            "node_modules",
            "build",
            "dist",
        }
        forbidden_dir_names = {
            "node_modules",
            ".venv",
            "venv",
            "__pycache__",
            ".pytest_cache",
            ".mypy_cache",
            ".ruff_cache",
        }
        forbidden_suffixes = {
            ".zip",
            ".7z",
            ".rar",
            ".sqlite",
            ".sqlite3",
            ".db",
            ".log",
            ".key",
            ".sealed",
            ".pem",
            ".p12",
            ".pfx",
        }
        bad = []
        for path in tracked_files():
            relative = path.relative_to(ROOT)
            if not relative.parts:
                continue
            if relative.parts[0] in forbidden_top_level or ".import" in relative.parts:
                bad.append(str(relative))
                continue
            if any(part in forbidden_dir_names for part in relative.parts[:-1]):
                bad.append(str(relative))
                continue
            if path.suffix.lower() in forbidden_suffixes:
                bad.append(str(relative))
                continue
            if path.name.startswith(".env") and path.name != ".env.example":
                bad.append(str(relative))
        self.assertEqual(bad, [], f"Forbidden tracked artifacts: {bad}")

    def test_mt5_gateway_remains_read_only(self):
        gateway = (ROOT / "gateway" / "mt5_gateway.py").read_text(encoding="utf-8")
        forbidden = (
            "order_send(",
            "order_check(",
            "TRADE_ACTION_",
            "position_close(",
        )
        found = [token for token in forbidden if token in gateway]
        self.assertEqual(found, [], f"Broker-write API appeared in gateway: {found}")
        self.assertIn("positions_get()", gateway)
        self.assertIn("orders_get()", gateway)

    def test_learning_pipeline_is_wired_in_causal_order(self):
        runtime = (ROOT / "crates" / "runtime" / "src" / "lib.rs").read_text(
            encoding="utf-8"
        )
        for module in (
            "mod learning;",
            "mod learning_review;",
            "mod learning_inference;",
            "mod learning_scoring;",
        ):
            self.assertIn(module, runtime)
        observe = runtime.index("self.learning.observe")
        inference = runtime.index("self.inference.observe")
        scoring = runtime.index("self.scoring.observe")
        publish = runtime.index("self.research=next")
        self.assertLess(observe, inference)
        self.assertLess(inference, scoring)
        self.assertLess(scoring, publish)
        self.assertIn('/v1/learning/inference', runtime)
        self.assertIn('/v1/learning/scoring', runtime)

    def test_learned_models_have_no_execution_or_promotion_route(self):
        runtime_dir = ROOT / "crates" / "runtime" / "src"
        source = "\n".join(
            path.read_text(encoding="utf-8", errors="ignore")
            for path in runtime_dir.rglob("*.rs")
        )
        for forbidden in (
            '/v1/learning/promote',
            '/v1/learning/champion/activate',
            '/v1/execute',
            '/v1/order',
            'order_send(',
        ):
            self.assertNotIn(forbidden, source)
        self.assertIn('promotion_enabled', source)
        self.assertIn('shadow_effect', source)

    def test_feature_and_label_schema_contract_is_shared(self):
        trainer = (ROOT / "scripts" / "train_linear_baseline.py").read_text(
            encoding="utf-8"
        )
        inference = (
            ROOT / "crates" / "runtime" / "src" / "learning_inference.rs"
        ).read_text(encoding="utf-8")
        scoring = (
            ROOT / "crates" / "runtime" / "src" / "learning_scoring.rs"
        ).read_text(encoding="utf-8")
        for text in (trainer, inference):
            self.assertIn("gold-m1-feature-v1", text)
            self.assertIn("braintrade-linear-softmax-v1", text)
        for text in (trainer, scoring):
            self.assertIn("gold-m1-outcome-v1", text)
        fixture = json.loads(
            (ROOT / "tests" / "learning_feature_parity.json").read_text(
                encoding="utf-8"
            )
        )
        self.assertEqual(fixture["features"]["schema"], "gold-m1-feature-v1")
        self.assertEqual(fixture["expected_feature_count"], 87)

    def test_repository_json_is_parseable(self):
        json_files = list((ROOT / "tests").glob("*.json")) + list(
            (ROOT / "docs").glob("*.json")
        )
        self.assertTrue(json_files)
        for path in json_files:
            with self.subTest(path=path.name):
                json.loads(path.read_text(encoding="utf-8"))

    def test_windows_ci_covers_development(self):
        workflow = (
            ROOT / ".github" / "workflows" / "windows.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("development", workflow)


if __name__ == "__main__":
    unittest.main()
