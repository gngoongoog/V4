import json
import re
import tomllib
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


def load_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


class ReleaseConsistencyTests(unittest.TestCase):
    def test_promoted_release_evidence_remains_verified(self):
        release = load_json(ROOT / "docs" / "validation.json")
        archived = load_json(ROOT / "docs" / "validation-v0.6.0.json")
        self.assertEqual(release, archived)
        self.assertEqual(release["version"], "0.6.0")
        self.assertRegex(release["tested_commit"], r"^[0-9a-f]{40}$")
        self.assertEqual(release["research_ci"]["conclusion"], "success")
        self.assertEqual(release["windows_ci"]["conclusion"], "success")
        self.assertIn(release["version"], (ROOT / "docs" / "RELEASE_STATUS.md").read_text(encoding="utf-8"))

    def test_development_version_and_state_documents_agree(self):
        version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
        development = load_json(ROOT / "docs" / "development-validation.json")
        release = load_json(ROOT / "docs" / "validation.json")
        self.assertEqual(development["version"], version)
        self.assertEqual(development["release_version"], release["version"])
        self.assertNotEqual(development["version"], release["version"])
        for path in (ROOT / "BUILD_HANDOFF.md", ROOT / "PROJECT_STATE.md", ROOT / "CONTINUE_HERE.md", ROOT / "ROADMAP.md"):
            text = path.read_text(encoding="utf-8")
            self.assertIn(version, text, path)
        self.assertIn(development["status"], {"pending-ci", "verified"})
        if development["status"] == "pending-ci":
            self.assertIsNone(development["tested_commit"])
            self.assertIsNone(development["tested_tree"])
            self.assertIsNone(development["research_ci"])
            self.assertIsNone(development["windows_ci"])
            self.assertEqual(development["windows_artifacts"], [])
        else:
            self.assertRegex(development["tested_commit"], r"^[0-9a-f]{40}$")
            self.assertRegex(development["tested_tree"], r"^[0-9a-f]{40}$")
            self.assertEqual(development["research_ci"]["conclusion"], "success")
            self.assertEqual(development["windows_ci"]["conclusion"], "success")

    def test_release_boundaries_remain_explicit(self):
        for filename in ("validation.json", "development-validation.json"):
            boundary = load_json(ROOT / "docs" / filename)["execution_boundary"]
            self.assertEqual(boundary["startup_restart_state"], "STOPPED")
            self.assertTrue(boundary["simulated_acceptance_only"])
            self.assertEqual(boundary["native_mt5_acceptance"], "not performed")
            self.assertEqual(boundary["telegram_live_delivery"], "not performed")
            self.assertFalse(boundary["imported_ml_automatic_promotion"])

    def test_roadmap_does_not_reintroduce_stale_execution_claim(self):
        roadmap = (ROOT / "ROADMAP.md").read_text(encoding="utf-8")
        self.assertNotIn("No broker execution exists.", roadmap)
        self.assertIn("native MT5 demo acceptance", roadmap)

    def test_packager_derives_version_from_workspace(self):
        source = (ROOT / "scripts" / "package-portable.py").read_text(encoding="utf-8")
        self.assertIn('VERSION = tomllib.loads((ROOT / "Cargo.toml")', source)
        self.assertIsNone(re.search(r'^VERSION\s*=\s*["\']\d', source, re.MULTILINE))

    def test_windows_preview_artifact_upload_is_version_agnostic(self):
        source = (ROOT / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")
        self.assertIn("name: BrainTrade-Windows-x64-Preview", source)
        self.assertIn("path: release/BrainTrade-Windows-x64-v*-Preview.zip", source)
        self.assertIsNone(re.search(r"BrainTrade-Windows-x64-v\d+\.\d+\.\d+", source))

    def test_no_audit_placeholders_are_tracked(self):
        self.assertFalse((ROOT / "docs" / "NOOP_DO_NOT_USE").exists())
        self.assertFalse((ROOT / "docs" / ".audit-tmp-ignore-placeholder").exists())


if __name__ == "__main__":
    unittest.main()
