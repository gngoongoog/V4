from concurrent.futures import ThreadPoolExecutor
import io
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'gateway'))
from bounded_log import BoundedStderr, bounded_stderr


class BoundedGatewayLogTests(unittest.TestCase):
    def test_existing_large_log_retains_only_bounded_previous_tail(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'gateway.log'
            old = b'x' * 4096 + b'last diagnostics\n'
            path.write_bytes(old)
            with path.open('a', encoding='utf-8') as stream:
                log = BoundedStderr(stream, path, 512)
                self.assertEqual(path.with_suffix('.log.1').read_bytes(), old[-512:])
                self.assertEqual(path.stat().st_size, 0)
                log.write('current process\n')
                self.assertEqual(path.read_text(), 'current process\n')
                self.assertFalse(stream.closed)

    def test_unicode_concurrent_writers_and_oversized_write_are_bounded(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'gateway.log'
            with path.open('a', encoding='utf-8') as stream:
                log = BoundedStderr(stream, path, 1024)
                with ThreadPoolExecutor(max_workers=4) as pool:
                    list(pool.map(log.write, ['تشخيص سليم\n' * 20] * 100))
                log.write('final record\n')
                self.assertLessEqual(path.stat().st_size, 1024)
                self.assertLessEqual(path.with_suffix('.log.1').stat().st_size, 1024)
                self.assertTrue(path.read_text().endswith('final record\n'))
                self.assertEqual(log.dropped_characters, 0)
            self.assertEqual(sorted(p.name for p in Path(directory).iterdir()),
                             ['gateway.log', 'gateway.log.1'])

    def test_archive_failure_keeps_active_log_and_never_grows_or_raises(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'gateway.log'
            with path.open('a', encoding='utf-8') as stream:
                log = BoundedStderr(stream, path, 512)
                log.write('a' * 512)
                with patch('bounded_log.os.replace', side_effect=PermissionError('locked archive')):
                    self.assertEqual(log.write('not written'), 11)
                self.assertEqual(path.read_text(), 'a' * 512)
                self.assertEqual(log.dropped_characters, 11)
                log.retry_after = 0
                log.write('recovered\n')
                self.assertEqual(path.read_text(), 'recovered\n')
                self.assertEqual(path.with_suffix('.log.1').read_text(), 'a' * 512)
            self.assertFalse(list(Path(directory).glob('*.tmp')))

    def test_console_and_other_files_are_not_modified(self):
        memory = io.StringIO()
        self.assertIs(bounded_stderr(memory, Path('gateway.log')), memory)
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / 'broker-journal.sqlite'
            journal.write_bytes(b'untouchable')
            with journal.open('a', encoding='utf-8') as stream:
                self.assertIs(bounded_stderr(stream, Path(directory) / 'gateway.log'), stream)
            self.assertEqual(journal.read_bytes(), b'untouchable')

    def test_startup_archive_failure_keeps_bounded_writer_installed(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'gateway.log'
            path.write_bytes(b'a' * 1024)
            with path.open('a', encoding='utf-8') as stream:
                with patch('bounded_log.os.replace', side_effect=PermissionError('locked archive')):
                    log = BoundedStderr(stream, path, 512)
                log.write('must not grow')
                self.assertEqual(path.stat().st_size, 1024)
                self.assertEqual(log.dropped_characters, 13)

    @unittest.skipUnless(os.name == 'nt', 'Windows append-only handle semantics')
    def test_windows_append_only_handle_can_rotate_without_renaming_active_file(self):
        import ctypes
        from ctypes import wintypes
        import msvcrt
        kernel = ctypes.WinDLL('kernel32', use_last_error=True)
        kernel.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD,
            wintypes.DWORD, ctypes.c_void_p, wintypes.DWORD, wintypes.DWORD,
            wintypes.HANDLE]
        kernel.CreateFileW.restype = wintypes.HANDLE
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'gateway.log'
            path.write_bytes(b'a' * 512)
            handle = kernel.CreateFileW(str(path), 0x00000004 | 0x00000080,
                0x1 | 0x2, None, 3, 0x80, None)
            self.assertNotEqual(handle, wintypes.HANDLE(-1).value)
            fd = msvcrt.open_osfhandle(handle, os.O_WRONLY | os.O_APPEND)
            with os.fdopen(fd, 'a', encoding='utf-8') as stream:
                log = BoundedStderr(stream, path, 512)
                log.write('new line\n')
                self.assertEqual(log.dropped_characters, 0)
                self.assertEqual(path.read_text(), 'new line\n')
                self.assertEqual(path.with_suffix('.log.1').read_bytes(), b'a' * 512)

    def test_active_path_replaced_does_not_truncate_new_file(self):
        # Windows deliberately disallows renaming the active handle. Use a
        # changed identity check to cover that same rejection on both systems.
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'gateway.log'
            with path.open('a', encoding='utf-8') as stream:
                log = BoundedStderr(stream, path, 512)
                log.write('a' * 512)
                with patch.object(log, '_owned_regular_file', return_value=False):
                    log.write('changed identity')
                self.assertEqual(path.read_text(), 'a' * 512)
                self.assertFalse(path.with_suffix('.log.1').exists())


if __name__ == '__main__':
    unittest.main()
