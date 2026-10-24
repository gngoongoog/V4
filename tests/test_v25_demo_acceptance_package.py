"""P0-B/G2: package and read-only Demo acceptance end-to-end staging contracts.

Synthetic staged directory only; tests NEVER initialize MT5, start Core,
execute GUI, read IPC tokens or submit broker orders.
"""
import importlib.util
from pathlib import Path
import shutil
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "p0b_check", ROOT / "scripts/check_v25_acceptance_package.py")
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)


class DemoAcceptancePortableContract(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.portable = Path(self.temp.name) / "Relocated V25 With Spaces"
        for item in check.REQUIRED_TOOLS:
            destination = self.portable / item
            destination.parent.mkdir(parents=True, exist_ok=True)
            source = {
                "Run-Demo-Acceptance.cmd": ROOT / "scripts/Run-Demo-Acceptance.cmd",
                "Tools/Run-Demo-Acceptance.ps1": ROOT / "scripts/Run-Demo-Acceptance.ps1",
                "Tools/Run-Demo-Acceptance-GUI.ps1": ROOT / "scripts/Run-Demo-Acceptance-GUI.ps1",
                "DEMO_ACCEPTANCE_AR.md": ROOT / "docs/DEMO_ACCEPTANCE_AR.md",
                "runtime/gateway/check_runtime.py": ROOT / "gateway/check_runtime.py",
                "runtime/gateway/execution.py": ROOT / "gateway/execution.py",
            }.get(item)
            if source:
                shutil.copy2(source, destination)
            else:
                destination.write_bytes(b"placeholder (never executed)")

    def test_readonly_acceptance_bundle_is_complete_after_relocation(self):
        result = check.verify(self.portable)
        failures = {name: item for name, item in result["checks"].items()
                    if item["status"] != "PASS"}
        self.assertEqual(failures, {})
        self.assertEqual(result["broker_demo_roundtrip"], "NOT TESTED")
        self.assertEqual(result["ea_metaeditor_compile"], "NOT TESTED")
        self.assertFalse(result["ready_for_real"])

    def test_missing_cli_gui_or_collector_fails_release(self):
        for relative in ("Tools/Run-Demo-Acceptance.ps1",
                         "Tools/Run-Demo-Acceptance-GUI.ps1",
                         "runtime/gateway/check_runtime.py"):
            target = self.portable / relative
            previous = target.read_bytes()
            target.unlink()
            with self.subTest(relative=relative):
                result = check.verify(self.portable)
                self.assertEqual(result["checks"]["asset:" + relative]["status"], "FAIL")
            target.write_bytes(previous)

    def test_launcher_injecting_order_send_or_remote_network_call_fails(self):
        target = self.portable / "Tools/Run-Demo-Acceptance.ps1"
        original = target.read_text(encoding="utf-8")
        for payload in ("\norder_send(user)\n", "\nInvoke-WebRequest http://example.com\n",
                        "\n/v1/start\n"):
            target.write_text(original + payload, encoding="utf-8")
            with self.subTest(payload=payload):
                self.assertEqual(check.verify(self.portable)["checks"][
                    "contract:Tools/Run-Demo-Acceptance.ps1"]["status"], "FAIL")
        target.write_text(original, encoding="utf-8")
        self.assertEqual(check.verify(self.portable)["checks"][
            "contract:Tools/Run-Demo-Acceptance.ps1"]["status"], "PASS")

    def test_staging_and_windows_build_include_release_gate(self):
        source = (ROOT / "scripts/package_v25.py").read_text("utf-8")
        windows = (ROOT / "scripts/Build-V25-Portable-Windows.ps1").read_text("utf-8")
        for token in ("Run-Demo-Acceptance.cmd", "Run-Demo-Acceptance.ps1",
                      "Run-Demo-Acceptance-GUI.ps1", "DEMO_ACCEPTANCE_AR.md"):
            self.assertIn(token, source)
        self.assertIn("check_v25_acceptance_package.py", windows)
        self.assertIn("demo-acceptance-relocated.json", windows)
        self.assertIn("Move-Item $portable $moved", windows)


if __name__ == "__main__":
    unittest.main()
