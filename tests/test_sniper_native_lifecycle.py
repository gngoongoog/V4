from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).parents[1]
sys.path.insert(0, str(ROOT / "gateway"))

import sniper_native as native


class _CooperativeThread:
    def __init__(self):
        self.native_id = 42
        self.alive = True
        self.join_calls = 0

    def is_alive(self):
        return self.alive

    def join(self, timeout=None):
        self.join_calls += 1
        self.alive = False


class _StuckServer:
    running = True

    def stop(self):
        return False


class NativeSniperLifecycleTests(unittest.TestCase):
    def test_stop_cancels_blocking_io_and_joins_worker(self):
        mailbox = native.NativeTickMailbox()
        mailbox.set_connected(True)
        server = native._WindowsPipeServer("token", mailbox)
        worker = _CooperativeThread()
        server._thread = worker
        cancellations = []
        server._cancel_synchronous_io = lambda: cancellations.append(True)

        self.assertTrue(server.stop())
        self.assertTrue(cancellations)
        self.assertGreaterEqual(worker.join_calls, 1)
        self.assertIsNone(server._thread)
        self.assertFalse(mailbox.connected)

    def test_close_refuses_to_rotate_token_while_old_server_is_stuck(self):
        with tempfile.TemporaryDirectory() as tmp:
            bridge = native.NativeTickBridge()
            bridge.server = _StuckServer()
            bridge.data_path = Path(tmp)
            bridge.token = "old-token"
            bridge.token_path = native.write_session_token(tmp, bridge.token)

            self.assertFalse(bridge.close())
            self.assertIsNotNone(bridge.server)
            self.assertEqual(bridge.token, "old-token")
            self.assertTrue(bridge.token_path.is_file())

    def test_running_property_does_not_report_dead_worker(self):
        server = native._WindowsPipeServer("token", native.NativeTickMailbox())
        worker = _CooperativeThread()
        worker.alive = False
        server._thread = worker
        self.assertFalse(server.running)


if __name__ == "__main__":
    unittest.main()
