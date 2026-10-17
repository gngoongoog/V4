from pathlib import Path
import unittest

ROOT = Path(__file__).parents[1]


class DemoAcceptanceOperatorTests(unittest.TestCase):
    def test_powershell_launcher_wraps_only_read_only_collector(self):
        source = (ROOT / "scripts" / "Run-Demo-Acceptance.ps1").read_text(encoding="utf-8")
        self.assertIn("--demo-acceptance", source)
        self.assertIn("--source", source)
        self.assertIn("--watch-seconds", source)
        self.assertIn("--minimum-fast-samples", source)
        self.assertIn("runtime-import-check.json", source)
        self.assertIn("ConvertFrom-Json", source)
        self.assertIn("queue_to_send_ms", source)
        self.assertIn("fast_path.sources.native", source)
        self.assertIn("fast_path.sources.poll", source)
        for forbidden in (
            "Invoke-RestMethod",
            "Invoke-WebRequest",
            "/v1/start",
            "/v1/stop",
            "/v1/execution/result",
            "order_send(",
            "MetaTrader5.initialize",
        ):
            self.assertNotIn(forbidden, source)

    def test_gui_is_only_a_frontend_for_the_read_only_cli(self):
        source = (ROOT / "scripts" / "Run-Demo-Acceptance-GUI.ps1").read_text(encoding="utf-8")
        self.assertIn("Run-Demo-Acceptance.ps1", source)
        self.assertIn("demo-acceptance-latest.json", source)
        self.assertIn("queue_to_send_ms", source)
        self.assertIn("Start-Process", source)
        self.assertIn("-NonInteractive", source)
        self.assertIn("READ ONLY", source)
        for forbidden in (
            "Invoke-RestMethod",
            "Invoke-WebRequest",
            "/v1/start",
            "/v1/stop",
            "/v1/execution/authorize",
            "order_send(",
            "MetaTrader5.initialize",
        ):
            self.assertNotIn(forbidden, source)

    def test_cmd_wrapper_opens_gui_without_args_and_preserves_cli_with_args(self):
        source = (ROOT / "scripts" / "Run-Demo-Acceptance.cmd").read_text(encoding="utf-8")
        self.assertIn("Tools\\Run-Demo-Acceptance.ps1", source)
        self.assertIn("Tools\\Run-Demo-Acceptance-GUI.ps1", source)
        self.assertIn('if "%~1"==""', source)
        self.assertIn("powershell.exe", source)
        self.assertNotIn("python.exe", source)
        self.assertNotIn("order_send", source)


if __name__ == "__main__":
    unittest.main()
