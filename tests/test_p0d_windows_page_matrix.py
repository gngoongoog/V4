"""P0-D 22-page source route/IPC matrix and regression for missing actions.

Tests inspect committed code only. They cannot run a real WebView2 window.
"""
from pathlib import Path
import importlib.util
import shutil
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location("page_matrix",ROOT/"scripts"/"audit_v25_windows_pages.py")
matrix=importlib.util.module_from_spec(spec)
spec.loader.exec_module(matrix)


class P0DPageMatrixTests(unittest.TestCase):
    def test_exactly_22_pages_all_allowed_routes_and_wired_ipc(self):
        report=matrix.check(ROOT)
        self.assertEqual(len(report["pages"]),22)
        self.assertEqual({item["id"] for item in report["pages"]},set(matrix.EXPECTED))
        failed={name:v for name,v in report["checks"].items() if v["status"]!="PASS"}
        self.assertEqual(failed,{})
        self.assertEqual(report["windows_webview2_clicks"],"NOT TESTED")
        self.assertEqual(report["22_page_visual_review"],"NOT TESTED")
        self.assertFalse(report["real_trading_ready"])

    def fixture(self):
        temp=tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        root=Path(temp.name)
        for relative in ("apps/terminal/src/terminal.tsx","apps/terminal/src/types.ts",
                         "apps/terminal/src/bridge.ts","apps/tauri/src/bridge.rs"):
            (root/relative).parent.mkdir(parents=True,exist_ok=True)
            shutil.copy2(ROOT/relative,root/relative)
        return root

    def test_missing_core_route_fails_release(self):
        root=self.fixture()
        target=root/"apps/tauri/src/bridge.rs"
        original=target.read_text("utf-8")
        target.write_text(original.replace('"/v1/news" |',''),encoding="utf-8")
        result=matrix.check(root)
        self.assertEqual(result["checks"]["all_pages_have_allowed_tauri_core_route"]["status"],"FAIL")

    def test_duplicate_page_or_navigation_not_reachable_fails_release(self):
        root=self.fixture()
        target=root/"apps/terminal/src/terminal.tsx"
        original=target.read_text("utf-8")
        altered=original.replace("{id:'news',", "{id:'events',")
        target.write_text(altered,encoding="utf-8")
        self.assertEqual(matrix.check(root)["checks"]["22_unique_page_registry"]["status"],"FAIL")
        target.write_text(original.replace("allOpen||i<11||x.id===page","allOpen||i<11"),encoding="utf-8")
        self.assertEqual(matrix.check(root)["checks"]["navigation_actions_for_all_pages"]["status"],"FAIL")
        target.write_text(original.replace("...PAGES.filter(x=>!primaryNav.includes(x.id))","...[]"),encoding="utf-8")
        self.assertEqual(matrix.check(root)["checks"]["navigation_actions_for_all_pages"]["status"],"FAIL")

    def test_stale_old_account_render_fence_removal_fails_release(self):
        root=self.fixture()
        target=root/"apps/terminal/src/terminal.tsx"
        original=target.read_text("utf-8")
        target.write_text(original.replace("pagePanel?.scope===viewScope?pagePanel.data:null",
                                           "pagePanel?.data??null"),encoding="utf-8")
        self.assertEqual(matrix.check(root)["checks"]["source_scoped_render_fence"]["status"],"FAIL")

    def test_unregistered_tauri_action_fails_release(self):
        root=self.fixture()
        target=root/"apps/terminal/src/bridge.ts"
        original=target.read_text("utf-8")
        target.write_text(original+"\nexport const debug=()=>invoke('steal_secrets');\n",encoding="utf-8")
        self.assertEqual(matrix.check(root)["checks"]["all_frontend_actions_registered_in_tauri"]["status"],"FAIL")


if __name__=="__main__":unittest.main()
