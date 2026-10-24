"""P0-E Native EA files and compilation helper must survive Windows Portable relocation.

All tests operate on synthetic files. No MetaEditor or MT5 process is started.
"""
from pathlib import Path
import importlib.util
import shutil
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location("native_package",ROOT/"scripts"/"check_v25_native_ea_package.py")
package=importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class NativePortablePackageTests(unittest.TestCase):
    def setUp(self):
        tmp=tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.directory=Path(tmp.name)/"Relocated V25 Folder With Spaces"
        sources={
            "mt5/Experts/BrainTradeSniperBridge.mq5":ROOT/"mt5"/"BrainTradeSniperBridge.mq5",
            "Tools/Compile-Sniper-EA.ps1":ROOT/"scripts"/"Compile-Sniper-EA.ps1",
            "Compile-Sniper-EA.cmd":ROOT/"scripts"/"Compile-Sniper-EA.cmd",
            "NATIVE_EA_ACCEPTANCE_AR.md":ROOT/"docs"/"v25"/"NATIVE_EA_ACCEPTANCE_AR.md",
        }
        for relative,src in sources.items():
            dest=self.directory/relative
            dest.parent.mkdir(parents=True,exist_ok=True)
            shutil.copy2(src,dest)

    def test_native_ea_package_is_complete_but_not_broker_validated(self):
        report=package.verify(self.directory)
        self.assertTrue(all(row["status"]=="PASS" for row in report["checks"].values()),
                        report["checks"])
        self.assertEqual(report["ea_metaeditor_compile"],"NOT TESTED")
        self.assertEqual(report["native_pipe_windows_demo"],"NOT TESTED")
        self.assertFalse(report["ready_for_real"])

    def test_missing_ea_source_or_metaeditor_helper_fails_release(self):
        for item in package.REQUIRED:
            p=self.directory/item
            original=p.read_bytes()
            p.unlink()
            with self.subTest(name=item):
                self.assertEqual(package.verify(self.directory)["checks"]["asset:"+item]["status"],"FAIL")
            p.write_bytes(original)

    def test_mql5_broker_order_call_never_accepted_as_tick_only_bridge(self):
        ea=self.directory/package.REQUIRED[0]
        original=ea.read_text("utf-8")
        ea.write_text(original+"\nOrderSend(request,reply);\n",encoding="utf-8")
        report=package.verify(self.directory)
        self.assertEqual(report["checks"]["contract:"+package.REQUIRED[0]]["status"],"FAIL")

    def test_compiler_must_be_explicit_and_cannot_fake_stale_ex5_success(self):
        src=(ROOT/"scripts/Compile-Sniper-EA.ps1").read_text("utf-8")
        self.assertIn("if ($InspectOnly)",src)
        self.assertIn("if ($InspectOnly) { exit 0 }",src)
        self.assertIn("if ($old -ne $new)",src)
        self.assertIn("LastWriteTimeUtc -ge $started.AddSeconds(-2)",src)
        self.assertIn("$file.Length -gt 1024",src)
        self.assertIn("ready_for_real = $false",src)
        self.assertIn("metaeditor_compiled = 'NOT TESTED'",src)
        self.assertNotIn("MetaTrader5.initialize",src)
        self.assertNotIn("OrderSend(",src)

    def test_stage_and_windows_build_have_canonical_ea_gates(self):
        package_source=(ROOT/"scripts/package_v25.py").read_text("utf-8")
        build=(ROOT/"scripts/Build-V25-Portable-Windows.ps1").read_text("utf-8")
        for term in ("Compile-Sniper-EA.ps1","Compile-Sniper-EA.cmd",
                     "NATIVE_EA_ACCEPTANCE_AR.md"):
            self.assertIn(term,package_source)
        self.assertIn("scripts/check_v25_native_ea_package.py",build)
        self.assertIn("native-ea-package.json",build)
        self.assertIn("native-ea-relocated.json",build)


if __name__=="__main__":unittest.main()
