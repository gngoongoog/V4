"""P1-K5J acceptance: SHA verifier explains *what* failed, not 973 files."""
from __future__ import annotations
from hashlib import sha256
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SCRIPT=Path(__file__).resolve().parents[1]/"scripts/v25_verify_system.py"
SPEC=importlib.util.spec_from_file_location("bt_v25_manifest_verifier",SCRIPT)
assert SPEC and SPEC.loader
VERIFIER=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFIER)

class ManifestDiagnostics(unittest.TestCase):
    def setUp(self):
        self.work=tempfile.TemporaryDirectory(prefix="braintrade-manifest-")
        self.addCleanup(self.work.cleanup)
        self.root=Path(self.work.name)
        (self.root/"runtime").mkdir()
        (self.root/"BrainTradeNext.exe").write_bytes(b"MZ"+b"frozen-test-binary")
        (self.root/"runtime/readme.txt").write_bytes(b"known")
        self.manifest={
            "BrainTradeNext.exe":sha256((self.root/"BrainTradeNext.exe").read_bytes()).hexdigest(),
            "runtime/readme.txt":sha256(b"known").hexdigest(),
        }
        (self.root/"SHA256SUMS.json").write_text(
            json.dumps(self.manifest),encoding="utf-8")

    def check(self):
        return VERIFIER.verify(self.root)["checks"]["sha256_manifest"]

    def test_clean_bundle_passes_with_explicit_zero_problem_counts(self):
        result=self.check()
        self.assertEqual(result["status"],"PASS")
        self.assertEqual(result["checked_count"],2)
        self.assertEqual(result["problems_total"],0)
        self.assertEqual(result["extra_unlisted_count"],0)

    def test_changed_missing_and_extra_are_reported_by_exact_relative_name(self):
        (self.root/"BrainTradeNext.exe").write_bytes(b"some-other-app")
        (self.root/"runtime/readme.txt").unlink()
        (self.root/"runtime/unknown.dll").write_bytes(b"untrusted")
        r=self.check()
        self.assertEqual(r["status"],"FAIL")
        self.assertEqual(r["checked_count"],2)
        self.assertEqual(r["problems_total"],3)
        self.assertEqual(r["missing_count"],1)
        self.assertEqual(r["changed_hash_count"],1)
        self.assertEqual(r["extra_unlisted_count"],1)
        self.assertIn("CHANGED HASH: BrainTradeNext.exe",r["problems"])
        self.assertIn("MISSING: runtime/readme.txt",r["problems"])
        self.assertIn("EXTRA UNLISTED: runtime/unknown.dll",r["problems"])

    def test_portable_python_does_not_mutate_signed_site_packages(self):
        root=Path(__file__).resolve().parents[1]
        core=(root/"crates/runtime/src/lib.rs").read_text(encoding="utf-8")
        launcher=(root/"VERIFY_SYSTEM.cmd").read_text(encoding="utf-8")
        self.assertIn('command.env("PYTHONDONTWRITEBYTECODE","1")',core)
        self.assertIn('set "PYTHONDONTWRITEBYTECODE=1"',launcher)
        self.assertIn('python.exe" -B ',launcher)

    def test_runtime_pycache_cannot_be_silently_ignored(self):
        (self.root/"runtime/__pycache__").mkdir()
        (self.root/"runtime/__pycache__/evil.cpython-313.pyc").write_bytes(b"untrusted")
        result=self.check()
        self.assertEqual(result["extra_unlisted_count"],1)
        self.assertEqual(result["status"],"FAIL")

    def test_html_report_includes_escaped_real_error_paths_and_total_count(self):
        (self.root/"runtime/extra<script>.py").write_bytes(b"tampered")
        out=self.root.parent/("diag-"+self.root.name)
        try:
            report=VERIFIER.verify(self.root)
            VERIFIER.write_report(report,out)
            page=(out/"v25-verification.html").read_text("utf-8")
            raw=json.loads((out/"v25-verification.json").read_text("utf-8"))
            self.assertIn("TOTAL PROBLEMS: 1",page)
            self.assertIn("EXTRA UNLISTED: runtime/extra&lt;script&gt;.py",page)
            self.assertNotIn("<script>",page)
            self.assertEqual(raw["checks"]["sha256_manifest"]["problems_total"],1)
        finally:
            import shutil
            shutil.rmtree(out,ignore_errors=True)

if __name__=="__main__":unittest.main()
