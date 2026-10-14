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

    def test_source_copy_defensively_excludes_secret_runtime_and_generated_state(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "source"
            destination = Path(tmp) / "copy"
            root.mkdir()
            (root / "keep.txt").write_text("safe", encoding="utf-8")
            (root / ".env.example").write_text("PLACEHOLDER=1", encoding="utf-8")
            for name in (
                ".env",
                "token.key",
                "telegram.sealed",
                "events.sqlite3",
                "runtime.db",
                "app.log",
                "cert.pem",
                "cert.p12",
                "cert.pfx",
                "old-source.zip",
                "old-source.7z",
                "old-source.rar",
            ):
                (root / name).write_text("secret", encoding="utf-8")
            runtime = root / "runtime"
            runtime.mkdir()
            (runtime / "generated.txt").write_text("generated", encoding="utf-8")
            generated_files = (
                ".ci/node_modules/pkg/index.js",
                "node_modules/pkg/index.js",
                ".venv/Lib/site-packages/pkg.py",
                "venv/Lib/site-packages/pkg.py",
                "src/__pycache__/module.pyc",
                "src/.pytest_cache/state.txt",
                "src/.mypy_cache/state.json",
                "src/.ruff_cache/state.bin",
                "vendor/node_modules/pkg/index.js",
            )
            for name in generated_files:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("generated", encoding="utf-8")
            original = packaging.ROOT
            packaging.ROOT = root
            try:
                packaging.source_copy(destination)
            finally:
                packaging.ROOT = original
            self.assertEqual((destination / "keep.txt").read_text(encoding="utf-8"), "safe")
            self.assertTrue((destination / ".env.example").is_file())
            self.assertFalse((destination / ".env").exists())
            self.assertFalse((destination / "token.key").exists())
            self.assertFalse((destination / "telegram.sealed").exists())
            self.assertFalse((destination / "events.sqlite3").exists())
            self.assertFalse((destination / "runtime.db").exists())
            self.assertFalse((destination / "app.log").exists())
            self.assertFalse((destination / "cert.pem").exists())
            self.assertFalse((destination / "cert.p12").exists())
            self.assertFalse((destination / "cert.pfx").exists())
            self.assertFalse((destination / "old-source.zip").exists())
            self.assertFalse((destination / "old-source.7z").exists())
            self.assertFalse((destination / "old-source.rar").exists())
            self.assertFalse((destination / "runtime").exists())
            for name in generated_files:
                self.assertFalse((destination / name).exists())

if __name__ == "__main__":
    unittest.main()
