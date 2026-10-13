import importlib.util
import hashlib
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("packaging", Path(__file__).parents[1]/"scripts/package-portable.py")
packaging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packaging)

class PackagingTests(unittest.TestCase):
    def test_manifest_rebuild_excludes_old_self_hash(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "BrainTrade.exe").write_bytes(b"fixture")
            (root / "SHA256SUMS.json").write_text('{"old":"manifest"}')
            expected = {"BrainTrade.exe": hashlib.sha256(b"fixture").hexdigest()}
            self.assertEqual(packaging.build_manifest(root), expected)
            (root / "SHA256SUMS.json").write_text('{"different":"manifest"}')
            self.assertEqual(packaging.build_manifest(root), expected)

    def test_rejects_path_traversal(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with zipfile.ZipFile(root/"bad.zip", "w") as z:
                z.writestr("../outside.txt", "bad")
            with self.assertRaises(ValueError):
                packaging.extract_checked(root/"bad.zip", root/"output")
            self.assertFalse((root/"outside.txt").exists())

    def test_rejects_tampered_cached_dependency_without_downloading(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root/"asset.zip").write_bytes(b"tampered")
            with self.assertRaisesRegex(ValueError, "hash mismatch"):
                packaging.download({"file": "asset.zip", "url": "https://invalid.example/", "sha256": "0"*64}, root)

    def test_preserves_package_layout(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with zipfile.ZipFile(root/"good.zip", "w") as z:
                z.writestr("Lib/site-packages/package/__init__.py", "value = 42\n")
            packaging.extract_checked(root/"good.zip", root/"output")
            self.assertEqual((root/"output/Lib/site-packages/package/__init__.py").read_text(), "value = 42\n")

if __name__ == "__main__":
    unittest.main()
