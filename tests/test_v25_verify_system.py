"""Read-only report verifier must detect missing/tampered content and never claim broker acceptance."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

HERE=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location("v25_verify_system",HERE/"scripts/v25_verify_system.py")
verify_module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify_module)


class VerificationContractTests(unittest.TestCase):
    def test_tampering_fails_and_unrun_acceptance_is_not_tested(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            for relative in verify_module.REQUIRED:
                path=root/relative
                path.parent.mkdir(parents=True,exist_ok=True)
                path.write_bytes(b"test-payload")
            manifest={rel:hashlib.sha256(b"test-payload").hexdigest() for rel in verify_module.REQUIRED}
            (root/"SHA256SUMS.json").write_text(json.dumps(manifest),encoding="utf-8")
            report=verify_module.verify(root)
            self.assertEqual(report["checks"]["sha256_manifest"]["status"],"PASS")
            self.assertEqual(report["checks"]["broker_demo_roundtrip"]["status"],"NOT TESTED")
            self.assertFalse(report["ready_for_real"])
            (root/"runtime/gateway/execution.py").write_text("tampered",encoding="utf-8")
            report=verify_module.verify(root)
            self.assertEqual(report["checks"]["sha256_manifest"]["status"],"FAIL")

    def test_manifest_path_traversal_is_rejected_without_reading_outside(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            (root/"SHA256SUMS.json").write_text(json.dumps({"../secrets.txt":"abcd"}),encoding="utf-8")
            report=verify_module.verify(root)
            self.assertEqual(report["checks"]["sha256_manifest"]["status"],"FAIL")
            self.assertFalse(report["ready_for_real"])

if __name__=="__main__":unittest.main()
