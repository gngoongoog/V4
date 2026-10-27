"""Offline shutdown regression: no MT5 terminal or broker is used."""
from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "gateway"))
from mt5_gateway import cleanup_gateway, watch_parent_stdin


class ParentShutdownTests(unittest.TestCase):
    def test_cleanup_attempts_all_steps_and_reports_a_stuck_pipe(self):
        calls = []
        class Item:
            def __init__(self, name, result=None, fail=False):
                self.name, self.result, self.fail = name, result, fail
            def close(self):
                calls.append(self.name)
                if self.fail:
                    raise OSError("fixture failure")
                return self.result
        class Terminal:
            def shutdown(self):
                calls.append("mt5")
        class Book:
            def release(self, _):
                calls.append("book")
        with self.assertRaisesRegex(RuntimeError, "transport, native pipe"):
            cleanup_gateway(Item("transport", fail=True), Item("native", result=False),
                            Item("journal"), Terminal(), Book(), Item("lock"))
        self.assertEqual(calls, ["transport", "native", "journal", "book", "mt5", "lock"])

    def test_pipe_eof_requests_shutdown_and_wakes_main_loop(self):
        reader_fd, writer_fd = os.pipe()
        wake = threading.Event()
        with os.fdopen(reader_fd, "rb", buffering=0) as reader:
            stopped = watch_parent_stdin(reader, wake.set)
            os.write(writer_fd, b"x")
            self.assertFalse(stopped.wait(0.05))
            os.close(writer_fd)
            self.assertTrue(stopped.wait(2), "EOF was not observed")
            self.assertTrue(wake.wait(2), "Native wait was not woken")

    def test_core_owned_gateway_process_exits_after_parent_pipe_closes(self):
        with tempfile.TemporaryDirectory(prefix="braintrade-gateway-shutdown-") as directory:
            root = Path(directory)
            # Even on a Windows machine with MT5 installed, this test cannot
            # initialize a terminal or send broker orders.
            (root / "MetaTrader5.py").write_text("raise ImportError('offline fixture')\n", encoding="utf-8")
            env = dict(os.environ, BRAINTRADE_DATA_DIR=directory,
                       BRAINTRADE_IPC_TOKEN="offline-fixture-token",
                       BRAINTRADE_GATEWAY_PARENT_STDIN="1",
                       PYTHONDONTWRITEBYTECODE="1", PYTHONPATH=directory)
            child = subprocess.Popen(
                [sys.executable, str(ROOT / "gateway" / "mt5_gateway.py")],
                env=env, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
            )
            try:
                deadline = time.monotonic() + 10
                while not (root / "gateway.lock").exists() and child.poll() is None and time.monotonic() < deadline:
                    time.sleep(0.05)
                self.assertTrue((root / "gateway.lock").exists(), "gateway did not initialize")
                self.assertIsNone(child.poll(), "gateway exited before shutdown")
                child.stdin.close()
                child.wait(timeout=10)
                self.assertEqual(child.returncode, 0, child.stderr.read().decode(errors="replace"))
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=5)
                if child.stdin and not child.stdin.closed:
                    child.stdin.close()
                if child.stderr:
                    child.stderr.close()


if __name__ == "__main__":
    unittest.main()
