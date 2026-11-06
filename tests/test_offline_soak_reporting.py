"""The endurance harness must not certify a run whose cleanup failed."""
from contextlib import contextmanager
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from check_offline_soak import temporary_case


class SoakReportingTests(unittest.TestCase):
    def test_cleanup_failure_replaces_pending_success_and_propagates(self):
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / 'result.json'
            report = {'status': 'CHECKS_PASSED_CLEANUP_PENDING'}

            @contextmanager
            def locked_directory(**kwargs):
                yield folder
                raise PermissionError('synthetic locked SQLite handle')

            with patch('check_offline_soak.tempfile.TemporaryDirectory', locked_directory):
                with self.assertRaises(PermissionError):
                    with temporary_case(report, output):
                        output.write_text(json.dumps(report))
            saved = json.loads(output.read_text())
            self.assertEqual(saved['status'], 'FAIL')
            self.assertIn('PermissionError', saved['error'])

    def test_body_failure_is_not_hidden_by_successful_cleanup(self):
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / 'result.json'
            report = {'status': 'RUNNING'}
            with self.assertRaisesRegex(RuntimeError, 'synthetic quote failure'):
                with temporary_case(report, output) as work:
                    self.assertTrue(Path(work).is_dir())
                    raise RuntimeError('synthetic quote failure')
            self.assertFalse(Path(work).exists())
            self.assertEqual(json.loads(output.read_text())['status'], 'FAIL')
