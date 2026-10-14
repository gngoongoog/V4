import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "source_archive.py"
spec = importlib.util.spec_from_file_location("braintrade_source_archive", SCRIPT)
archive_tool = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(archive_tool)


class SourceArchiveTests(unittest.TestCase):
    def test_archive_preserves_source_runtime_crate_and_excludes_state(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "source"
            output = Path(temporary) / "out" / "source.zip"
            (root / "crates" / "runtime" / "src").mkdir(parents=True)
            (root / "crates" / "runtime" / "src" / "lib.rs").write_text("safe", encoding="utf-8")
            (root / ".env.example").write_text("PLACEHOLDER=1", encoding="utf-8")
            (root / "runtime").mkdir()
            (root / "runtime" / "generated.txt").write_text("generated", encoding="utf-8")
            (root / ".import").mkdir()
            (root / ".import" / "chunk.txt").write_text("transport", encoding="utf-8")
            for name in (
                ".env",
                "ipc.key",
                "telegram.sealed",
                "events.sqlite3",
                "cache.db",
                "core.log",
                "cert.pem",
                "cert.p12",
                "cert.pfx",
            ):
                (root / name).write_text("secret", encoding="utf-8")

            size, digest = archive_tool.create_archive(root, output)
            self.assertGreater(size, 0)
            self.assertEqual(len(digest), 64)
            with zipfile.ZipFile(output) as archive:
                names = set(archive.namelist())
            self.assertIn("BrainTrade-OS/crates/runtime/src/lib.rs", names)
            self.assertIn("BrainTrade-OS/.env.example", names)
            self.assertNotIn("BrainTrade-OS/runtime/generated.txt", names)
            self.assertNotIn("BrainTrade-OS/.import/chunk.txt", names)
            for name in (
                ".env",
                "ipc.key",
                "telegram.sealed",
                "events.sqlite3",
                "cache.db",
                "core.log",
                "cert.pem",
                "cert.p12",
                "cert.pfx",
            ):
                self.assertNotIn("BrainTrade-OS/" + name, names)

    def test_output_inside_source_tree_never_archives_itself(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "source"
            root.mkdir()
            (root / "README.md").write_text("safe", encoding="utf-8")
            output = root / "snapshot.zip"
            size, digest = archive_tool.create_archive(root, output)
            self.assertGreater(size, 0)
            self.assertEqual(len(digest), 64)
            self.assertFalse((root / "snapshot.zip.tmp").exists())
            with zipfile.ZipFile(output) as archive:
                names = set(archive.namelist())
            self.assertEqual(names, {"BrainTrade-OS/README.md"})


if __name__ == "__main__":
    unittest.main()
