"""Static wiring regression for the native 20-page Slint dashboard.

This guards against a polished but dead UI: every navigation destination must
be renderable, every button must dispatch an action, and every declared Slint
callback must have a registered Rust handler. Rendering itself is separately
validated by the native --render-check suite in Research CI.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
SLINT = ROOT / "crates/desktop/ui/main.slint"
CORRELATION = ROOT / "crates/desktop/ui/correlation.slint"
RUST = ROOT / "crates/desktop/src"


class NativeUiContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.dashboard = SLINT.read_text(encoding="utf-8")
        cls.pages = cls.dashboard + "\n" + CORRELATION.read_text(encoding="utf-8")
        cls.rust = "\n".join(p.read_text(encoding="utf-8") for p in RUST.glob("*.rs"))

    def test_all_twenty_pages_have_a_visible_navigation_destination(self):
        visible = {int(i) for i in re.findall(r"if\s+root\.page\s*==\s*(\d+)", self.dashboard)}
        reachable = {int(i) for i in re.findall(r"root\.page\s*=\s*(\d+)", self.dashboard)}
        self.assertEqual(visible, set(range(20)), "All 20 compiled pages must be renderable")
        self.assertEqual(reachable, visible, "Navigation must be able to open all pages")
        for section in range(5):
            self.assertRegex(self.dashboard, rf"root\.nav-section\s*==\s*{section}")

    def test_no_button_is_rendered_without_a_click_handler(self):
        for path in (SLINT, CORRELATION):
            src = path.read_text(encoding="utf-8")
            # Every Button {} declaration must have a clicked event; bare or
            # decorative buttons are not accepted into the Windows release.
            count = len(re.findall(r"\bButton\s*\{", src))
            wired = len(re.findall(r"\bclicked\s*=>", src))
            self.assertGreater(count, 0)
            self.assertEqual(count, wired, f"Found inert control in {path.name}")

    def test_every_declared_callback_is_registered_in_the_desktop(self):
        callbacks = set(re.findall(r"(?m)^\s*callback\s+([a-z0-9-]+)\s*\(", self.dashboard))
        callbacks.update(re.findall(r"(?m)^\s*callback\s+([a-z0-9-]+)\s*\(", CORRELATION.read_text(encoding="utf-8")))
        # Correlation callback is forwarded to Dashboard; not a Rust callback itself.
        handlers = set(re.findall(r"\bui\.on_([a-z0-9_]+)\s*\(", self.rust))
        for callback in callbacks:
            rust_name = callback.replace("-", "_")
            if callback == "action":
                continue
            self.assertIn(rust_name, handlers, f"Slint callback has no Rust handler: {callback}")
        self.assertIn("action", handlers)

    def test_operator_flow_and_execution_controls_stay_wired(self):
        for label in ("1  ربط MT5", "2  تجهيز التداول", "3  متابعة التنفيذ",
                      "بدء النظام  START", "إيقاف  STOP", "خروج كامل",
                      "حفظ إعدادات التداول", "ربط الحساب الحالي",
                      "إرجاع للمنظومة", "حفظ حماية المحفظة"):
            self.assertIn(label, self.dashboard, f"Critical control disappeared: {label}")
        self.assertIn('root.action("start")', self.dashboard)
        self.assertIn('root.action("stop")', self.dashboard)
        self.assertIn('root.action("shutdown")', self.dashboard)
        self.assertIn('root.execution-action("bind")', self.dashboard)
        self.assertIn('root.execution-action("portfolio")', self.dashboard)
        self.assertIn('root.studio-action("save")', self.dashboard)
        self.assertIn('root.state == "STOPPED"', self.dashboard)
        self.assertIn("root.exec-busy", self.dashboard)

    def test_palette_remains_grayscale_including_research_subpage(self):
        colors = re.findall(r"#[0-9a-fA-F]{6}\b", self.pages)
        self.assertGreater(len(colors), 70)
        for color in colors:
            rgb = [int(color[i:i+2], 16) for i in (1, 3, 5)]
            self.assertLessEqual(max(rgb)-min(rgb), 19,
                                 f"Colored accent {color} breaks smoke/black/white palette")


if __name__ == "__main__":
    unittest.main()
