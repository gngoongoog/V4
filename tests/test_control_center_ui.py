from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
UI = ROOT / "crates" / "desktop" / "ui" / "main.slint"
STUDIO = ROOT / "crates" / "desktop" / "src" / "studio.rs"


class ControlCenterUiTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ui = UI.read_text(encoding="utf-8")
        cls.studio = STUDIO.read_text(encoding="utf-8")

    def test_navigation_is_grouped_into_five_primary_sections(self):
        self.assertIn('in-out property <int> nav-section: 0;', self.ui)
        for label in ("الرئيسية", "التداول", "البحث", "المختبر", "النظام"):
            self.assertIn(f'NavItem {{ text: "{label}"; selected: root.nav-section ==', self.ui)
        self.assertIn('if root.nav-section == 1:', self.ui)
        self.assertIn('if root.nav-section == 2:', self.ui)
        self.assertIn('if root.nav-section == 3:', self.ui)
        self.assertIn('if root.nav-section == 4:', self.ui)

    def test_system_section_exposes_backup_page(self):
        self.assertIn('selected: root.page == 13; text: "النسخ"', self.ui)
        self.assertIn('root.page = 13; root.backup-action("refresh")', self.ui)

    def test_sniper_hold_is_visible_and_saved(self):
        self.assertIn('in-out property <string> sniper-hold: "120";', self.ui)
        self.assertIn('text <=> root.sniper-hold', self.ui)
        self.assertIn('ui.set_sniper_hold(c.trading.sniper_max_hold_seconds.to_string().into())', self.studio)
        self.assertIn('sniper_max_hold_seconds:ui.get_sniper_hold().trim().parse()?', self.studio)
        self.assertNotIn('sniper_max_hold_seconds:120', self.studio)

    def test_commission_basis_is_user_selectable_and_forwarded(self):
        self.assertIn('in-out property <int> commission-basis: 0;', self.ui)
        self.assertIn('current-index <=> root.commission-basis', self.ui)
        self.assertIn('ui.get_commission_basis()==1', self.studio)
        self.assertGreaterEqual(self.studio.count('"commission_basis":basis'), 2)
        self.assertIn('ui.set_commission_basis(if v["commission_editor"]["basis"]=="per_side"{1}else{0})', self.studio)

    def test_trade_center_explains_frozen_open_trade_hold(self):
        self.assertIn('الصفقة المفتوحة تحتفظ بهدفها ووقفها ومدتها التي جُمّدت عند الدخول', self.ui)
        self.assertIn('مدة الاحتفاظ من 1 إلى 3600 ثانية', self.ui)

    def test_risk_editor_is_separate_from_per_side_trading_editor(self):
        self.assertIn('text <=> root.risk-commission-lot', self.ui)
        source = (ROOT / 'crates/desktop/src/main.rs').read_text(encoding='utf-8')
        self.assertIn('ui.get_risk_commission_lot(),', source)
        self.assertNotIn('ui.get_commission_lot(),', source)
        self.assertIn('root.action("risk_config")', self.ui)
        self.assertIn('Task::Action(a) if a == "risk_config"', source)

    def test_brain_selector_cannot_race_an_inflight_load(self):
        self.assertIn('ComboBox { enabled: !root.exec-busy; model: root.exec-names;', self.ui)


if __name__ == "__main__":
    unittest.main()
