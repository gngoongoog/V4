import importlib.util
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('release_identity', ROOT/'scripts/release_identity.py')
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseIdentityTests(unittest.TestCase):
    def test_binary_asset_bytes_are_included_without_line_ending_conversion(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root/'release-inputs.json').write_text('["image.jpg"]')
            data = b'\xff\xd8\xff\r\n\x80\x00'
            asset = root/'image.jpg'
            asset.write_bytes(data)
            name = b'image.jpg'
            expected = hashlib.sha256(b'braintrade-source-v1\0'
                + len(name).to_bytes(8, 'big') + name
                + len(data).to_bytes(8, 'big') + data).hexdigest()
            self.assertEqual(release.fingerprint(root), expected)
            asset.write_bytes(data.replace(b'\r\n', b'\n'))
            self.assertNotEqual(release.fingerprint(root), expected)

    def test_versions_and_identity(self):
        release.validate_versions()
        self.assertRegex(release.identity()['build_id'], '^[a-f0-9]{64}$')
        self.assertFalse(json.loads((ROOT/'release.json').read_text())['ready_for_real'])

    def test_source_change_invalidates_identity_and_windows_line_endings_do_not(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            (root/'release-inputs.json').write_text('["source"]')
            (root/'source').mkdir()
            f=root/'source/engine.py'
            f.write_bytes(b'one\ntwo\n')
            first=release.fingerprint(root)
            f.write_bytes(b'one\r\ntwo\r\n')
            self.assertEqual(first,release.fingerprint(root))
            (root/'README.md').write_text('documentation alone')
            (root/'source/__pycache__').mkdir()
            (root/'source/__pycache__/compiled.pyc').write_bytes(b'\xff')
            self.assertEqual(first,release.fingerprint(root))
            f.write_bytes(b'one\nchanged\n')
            self.assertNotEqual(first,release.fingerprint(root))
            f.unlink()
            self.assertNotEqual(first,release.fingerprint(root))
            (root/'release-inputs.json').write_text('["missing"]')
            with self.assertRaisesRegex(ValueError,'Missing release input'):
                release.fingerprint(root)
