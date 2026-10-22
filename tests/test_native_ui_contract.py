"""Static wiring regression for the native 22-page Slint dashboard.

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

    def test_all_twenty_two_pages_have_a_visible_navigation_destination(self):
        visible = {int(i) for i in re.findall(r"if\s+root\.page\s*==\s*(\d+)", self.dashboard)}
        reachable = {int(i) for i in re.findall(r"root\.page\s*=\s*(\d+)", self.dashboard)}
        self.assertEqual(visible, set(range(22)), "All 22 compiled pages must be renderable")
        self.assertEqual(reachable, visible, "Navigation must be able to open all pages")
        for section in range(5):
            self.assertRegex(self.dashboard, rf"root\.nav-section\s*==\s*{section}")

    def test_all_native_form_controls_have_real_two_way_bindings(self):
        # Beyond clickable buttons: verify each input persists into a property
        # or invokes a selection callback. This is source wiring, not a
        # replacement for interacting with native widgets on Windows.
        def controls(source, kind):
            pattern=re.compile(r"\b"+kind+r"\s*\{")
            for match in pattern.finditer(source):
                start=source.index("{",match.start())
                depth=0
                quoted=False
                escaped=False
                for offset in range(start,len(source)):
                    ch=source[offset]
                    if quoted:
                        if escaped:
                            escaped=False
                        elif ch=="\\":
                            escaped=True
                        elif ch=='"':
                            quoted=False
                    elif ch=='"':
                        quoted=True
                    elif ch=="{":
                        depth+=1
                    elif ch=="}":
                        depth-=1
                        if depth==0:
                            yield source[match.start():offset+1]
                            break
                else:
                    self.fail(f"Unclosed Slint {kind} control")
        all_sources=(self.dashboard,CORRELATION.read_text(encoding="utf-8"))
        total={"Button":0,"ComboBox":0,"LineEdit":0,"CheckBox":0}
        for code in all_sources:
            for kind in total:
                for element in controls(code,kind):
                    total[kind]+=1
                    if kind=="Button":
                        self.assertRegex(element,r"\bclicked\s*=>")
                    elif kind=="ComboBox":
                        self.assertRegex(element,r"(current-index\s*<=>|selected\s*(?:\([^)]*\))?\s*=>)")
                    elif kind=="LineEdit":
                        self.assertRegex(element,r"\btext\s*<=>")
                    else:
                        self.assertRegex(element,r"\bchecked\s*<=>")
        # Catch missing fields silently disappearing from nested layouts.
        self.assertEqual(total,{"Button":130,"ComboBox":29,"LineEdit":49,"CheckBox":12})

    def test_premium_native_dashboard_keeps_real_market_provenance(self):
        main=(RUST / "main.rs").read_text(encoding="utf-8")
        self.assertIn("COMMAND CENTER  /  مركز القيادة",self.dashboard)
        self.assertIn("PROFESSIONAL CHART",self.dashboard)
        self.assertIn("CORE INTELLIGENCE  /  العقل الرئيسي",self.dashboard)
        self.assertIn("MT5 TICK ACTIVITY",self.dashboard)
        self.assertIn("volume:float",self.dashboard)
        self.assertIn("bar.volume",self.dashboard)
        self.assertIn("max_tick_volume = subset.iter().map(|c| c.tick_volume).max()",main)
        self.assertIn("if max_tick_volume == 0",main)
        self.assertIn("market.valid()",main)
        self.assertIn("captured_ms <= 120_000",main) if "captured_ms <= 120_000" in main else self.assertIn("now_ms.saturating_sub(market.captured_ms) <= 120_000",main)

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
        for label in ("مركز التداول", "وضع القناص", "حالة العقل الرئيسي",
                      "▶  START", "■  STOP", "خروج كامل",
                      "حفظ إعدادات التداول", "ربط حساب Demo", "تأكيد وربط حساب REAL",
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

    def test_real_switch_has_two_separate_account_confirmations(self):
        self.assertIn('ComboBox { model: ["Demo / حساب تجريبي", "Live / أموال حقيقية"]',self.dashboard)
        self.assertIn("text <=> root.exec-confirm-login",self.dashboard)
        self.assertIn("text <=> root.live-start-login",self.dashboard)
        self.assertIn('root.exec-target == 0 || root.exec-confirm-login != ""',self.dashboard)
        self.assertIn('root.execution-action("bind")',self.dashboard)
        self.assertIn('root.action("start")',self.dashboard)
        self.assertIn("ui.get_exec_confirm_login()",self.rust)
        self.assertIn("ui.get_live_start_login()",self.rust)
        self.assertIn('ui.set_live_start_login("".into())',self.rust)

    def test_visible_desktop_build_label_matches_current_core_package_version(self):
        import tomllib
        version=tomllib.loads((ROOT/"Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
        self.assertIn("BRAINTRADE V4 — Institutional Windows Terminal",self.dashboard)
        self.assertIn("Windows "+version,self.dashboard)
        self.assertIn("BRAINTRADE OS "+version,self.dashboard)
        self.assertNotIn("Windows 0.7.1",self.dashboard)

    def test_compact_navigation_preserves_all_22_real_page_actions(self):
        self.assertIn("current-index <=> root.page",self.dashboard)
        self.assertIn("visible: root.expanded-workspaces",self.dashboard)
        self.assertIn('if root.account-mode-label == "REAL": HorizontalLayout',self.dashboard)
        self.assertIn("text <=> root.live-start-login",self.dashboard)
        self.assertIn("root.expanded-workspaces = !root.expanded-workspaces",self.dashboard)
        for action in ('root.studio-action("config")',
                       'root.execution-action("refresh")',
                       'root.trade-journal-action("refresh")',
                       'root.brain-monitor-action("refresh")',
                       'root.risk-monitor-refresh()',
                       'root.correlation-action("refresh")',
                       'root.action("events")'):
            self.assertIn(action,self.dashboard)
    def test_institutional_risk_brain_and_execution_cards_reuse_real_state(self):
        checks = (
            'label:"MODE / الوضع الحالي"',
            'value:root.trade-mode == 3 ? "SNIPER"',
            'label:"BROKER / نوع الحساب"',
            'value:root.account-mode-label',
            'label:"BRAIN MONITOR / حالة البيانات"',
            'value:root.brain-monitor-busy',
            'label:"EXECUTION / الحالة"',
            'value:root.exec-busy',
            'label:"RISK MONITOR / تحديث"',
            'value:root.risk-monitor-busy',
            'text: root.brain-monitor-detail',
            'text: root.risk-monitor-text',
            'root.brain-monitor-action("save")',
            'root.execution-action("bind")',
            'root.save-risk()'
        )
        for expression in checks:
            self.assertIn(expression, self.dashboard, expression)
        for index in range(22):
            self.assertIn(f'root.page == {index} ?',self.dashboard,
                          f"Missing per-page live breadcrumb for page {index}")

    def test_windows_gui_render_process_is_synchronous_before_screenshot_check(self):
        script=(ROOT / "scripts/build-windows.ps1").read_text(encoding="utf-8")
        self.assertIn("Start-Process -FilePath",script)
        self.assertIn("-PassThru -Wait",script)
        self.assertIn("-WorkingDirectory $PWD.Path",script)
        self.assertIn("$NativeRenderer.ExitCode -ne 0",script)
        self.assertLess(script.index("Start-Process -FilePath"),script.index("scripts\\check_native_ui_visuals.py"))

    def test_packaged_windows_visual_contract_accepts_all_three_palettes(self):
        portable=(ROOT / "scripts/check_windows_portable.py").read_text(encoding="utf-8")
        self.assertIn("review_pages = {0, 6, 14, 17, 18, 19, 21}",portable)
        self.assertIn('for palette in ("white", "smoke")',portable)
        self.assertIn('all 72 saved native theme frames',portable)
        self.assertNotIn('all 44 complete native frames',portable)

    def test_standalone_app_uses_smoke_as_default_without_overwriting_saved_theme(self):
        main=(RUST / "main.rs").read_text(encoding="utf-8")
        self.assertIn("else{return (2,false,false)}",main)
        self.assertIn(".unwrap_or((2,false,false))",main)
        self.assertIn("ui.set_theme_choice(initial_theme)",main)

    def test_active_sidebar_highlights_current_workspace(self):
        for page, title in ((0,"نظرة عامة"),(14,"مركز التداول"),(21,"العقول"),
                            (6,"إدارة المخاطر"),(17,"التنفيذ MT5"),
                            (20,"سجل الصفقات"),(19,"تقدّم التعلّم")):
            self.assertIn(f'(root.page == {page} ? "●   {title}"',self.dashboard)

    def test_visual_qa_is_enforced_in_research_and_windows_package(self):
        research=(ROOT / ".github/workflows/research.yml").read_text(encoding="utf-8")
        windows=(ROOT / "scripts/build-windows.ps1").read_text(encoding="utf-8")
        checker=(ROOT / "scripts/check_native_ui_visuals.py").read_text(encoding="utf-8")
        self.assertEqual(research.count("run: python3 scripts/check_native_ui_visuals.py"), 2)
        self.assertIn("scripts\\check_native_ui_visuals.py",windows)
        self.assertIn("for page in range(22)",checker)
        self.assertIn("if validated != 72",checker)
        self.assertIn("if len({dark, white, smoke}) != 3",checker)

    def test_canceling_studio_blocks_new_studio_actions_until_ack(self):
        src=(RUST / "studio.rs").read_text(encoding="utf-8")
        self.assertIn('if ui.get_studio_busy() || ui.get_studio_cancel_busy(){return;}',src)
        # At least all the buttons bound to the ordinary studio-busy guard
        # must also be disabled during cancellation; never replace the actual
        # Cancel button's separate busy state.
        self.assertNotIn('enabled: !root.studio-busy;',self.dashboard)
        self.assertIn('enabled: !root.studio-busy && !root.studio-cancel-busy',self.dashboard)
        self.assertIn('enabled: !root.studio-cancel-busy; clicked => { root.studio-action("cancel"); }',self.dashboard)

    def test_studio_cancel_really_works_while_studio_is_busy(self):
        src=(RUST / "studio.rs").read_text(encoding="utf-8")
        self.assertIn('if action=="cancel"',src)
        self.assertIn('ui.set_studio_cancel_busy(true)',src)
        self.assertIn('ui.set_studio_cancel_busy(false)',src)
        self.assertIn('"/v1/studio/cancel"',src)
        self.assertLess(src.index('if action=="cancel"'), src.index('if ui.get_studio_busy() || ui.get_studio_cancel_busy(){return;}'))
        self.assertIn('enabled: !root.studio-cancel-busy; clicked => { root.studio-action("cancel"); }',self.dashboard)
        self.assertIn('in property <bool> studio-cancel-busy: false;',self.dashboard)

    def test_status_strip_responsive_and_white_theme_legible(self):
        self.assertIn('visible: root.width >= 1100px;', self.dashboard)
        self.assertIn('visible: root.width >= 1250px;', self.dashboard)
        self.assertIn('visible: root.width >= 1400px;', self.dashboard)
        self.assertIn('#e9eff6', self.dashboard)
        self.assertIn('#dff5ee', self.dashboard)

    def test_global_page_refresh_is_only_wired_to_real_supported_handlers(self):
        for page, callback in (
            (2, 'root.action("events")'),
            (6, 'root.risk-monitor-refresh()'),
            (7, 'root.backtest-action("refresh")'),
            (8, 'root.news-action("refresh")'),
            (9, 'root.evaluation-action("refresh")'),
            (11, 'root.learning-action("refresh")'),
            (13, 'root.backup-action("refresh")'),
            (14, 'root.studio-action("config")'),
            (17, 'root.execution-action("refresh")'),
            (18, 'root.correlation-action("refresh")'),
            (20, 'root.trade-journal-action("refresh")'),
            (21, 'root.brain-monitor-action("refresh")'),
        ):
            self.assertIn(f"root.page == {page}", self.dashboard)
            self.assertIn(callback, self.dashboard)
        self.assertIn('text: "↻ تحديث الصفحة"', self.dashboard)
        self.assertIn("!root.news-refreshing", self.dashboard)
        self.assertIn("!root.brain-monitor-busy", self.dashboard)
        self.assertIn('root.page == 15 || root.page == 16', self.dashboard)
        self.assertIn("root.save-appearance(root.theme-choice,root.compact-sidebar,root.expanded-workspaces)", self.dashboard)

    def test_entire_22_page_windows_dashboard_supports_saved_appearance(self):
        main = (RUST / "main.rs").read_text(encoding="utf-8")
        render = (RUST / "render.rs").read_text(encoding="utf-8")
        correlation = CORRELATION.read_text(encoding="utf-8")
        for token in ("callback save-appearance(int, bool, bool)", "root.save-appearance(",
                      "changed theme-choice =>", "root.compact-sidebar"):
            self.assertIn(token, self.dashboard)
        self.assertIn("ui.on_save_appearance", main)
        self.assertIn('data_dir().join("ui-appearance.json")', main)
        self.assertIn("ui.set_theme_choice(initial_theme)", main)
        self.assertIn("ui.set_compact_sidebar(initial_compact)", main)
        self.assertIn("ui.set_expanded_workspaces(initial_expanded)", main)
        self.assertIn('"expanded_workspaces":expanded', main)
        self.assertIn("theme-choice: root.theme-choice", self.dashboard)
        self.assertIn("in property <int> theme-choice: 0;", correlation)
        for marker in ('[(0,"dark"),(1,"white"),(2,"smoke")]', "for page in 0..22",
                       "if theme == 0 || [0,6,14,17,18,19,21].contains(&page)"):
            self.assertIn(marker, render)
        # Zero-connection monitoring is explicitly unknown, not a fabricated score.
        self.assertIn("نسبة نجاح أو تعلّم: غير محسوبة", self.dashboard)

    def test_three_institutional_themes_and_real_market_chart_are_explicit(self):
        for mode, palette in (("داكن","ColorScheme.dark"),
                              ("أبيض","ColorScheme.dark"),
                              ("رمادي","ColorScheme.dark")):
            self.assertIn(f'text:"{mode}"',self.dashboard)
            self.assertIn(palette,self.dashboard)
        self.assertIn("theme-choice: 0",self.dashboard)
        self.assertIn("changed theme-choice => { Palette.color-scheme = ColorScheme.dark; }",self.dashboard)
        self.assertIn("root.chart-bars",self.dashboard)
        self.assertIn("بانتظار شموع MT5",self.dashboard)
        self.assertIn("QUOTE AGE / عمر السعر",self.dashboard)
        # A quote-age metric must not be marketed as broker execution latency.
        self.assertNotIn('Latency " + root.quote-age-display',self.dashboard)
        self.assertIn('root.execution-action("bind")',self.dashboard)



if __name__ == "__main__":
    unittest.main()
