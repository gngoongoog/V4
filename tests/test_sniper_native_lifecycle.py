import os
from pathlib import Path
import sys
import tempfile
import time
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

    def test_gateway_periodically_recovers_a_dead_pipe_without_resetting_mt5(self):
        source=(ROOT / "gateway" / "mt5_gateway.py").read_text(encoding="utf-8")
        self.assertIn("next_native_rebind=0.0",source)
        self.assertIn("if mono >= next_native_rebind:",source)
        self.assertIn("if not native.available:",source)
        self.assertIn("native.bind_terminal(mt5.terminal_info())",source)
        server_source=(ROOT / "gateway" / "sniper_native.py").read_text(encoding="utf-8")
        self.assertIn("def _serve(self)",server_source)
        self.assertIn("Native Sniper pipe",server_source)
        self.assertIn("_log_problem('CreateNamedPipeW'",server_source)

    def test_ea_waiting_status_identifies_stage_and_windows_io_error_without_token(self):
        ea=(ROOT / "mt5" / "BrainTradeSniperBridge.mq5").read_text(encoding="utf-8")
        self.assertIn('Last I/O error: ',ea)
        self.assertIn('Last I/O stage: ',ea)
        self.assertIn('Token file: ',ea)
        self.assertNotIn('\\nToken: ',ea)
        self.assertIn('g_last_io_error = GetLastError()',ea)
        self.assertNotIn('OrderSend(',ea)
        self.assertIn("SendPendingQuoteTicks();",ea)
        self.assertNotIn("if(!SendPendingQuoteTicks())",ea)
        self.assertIn("g_failed_ticks += forwarded;",ea)

    @unittest.skipUnless(os.name == "nt", "Windows named-pipe contract")
    def test_real_windows_blocking_connect_stops_and_same_name_rebinds(self):
        pipe_name = rf"\\.\pipe\BrainTradeSniperTest-{os.getpid()}-{id(self)}"
        first = native._WindowsPipeServer("token-a", native.NativeTickMailbox(), pipe_name)
        first.start()
        self.assertTrue(first.running)
        started = time.monotonic()
        self.assertTrue(first.stop())
        self.assertLess(time.monotonic() - started, 1.5)

        second = native._WindowsPipeServer("token-b", native.NativeTickMailbox(), pipe_name)
        second.start()
        self.assertTrue(second.running)
        self.assertTrue(second.stop())

    @unittest.skipUnless(os.name == "nt", "Windows named-pipe integration")
    def test_real_pipe_fragmented_authenticated_ticks_and_reconnect_freshness(self):
        pipe_name = rf"\\.\pipe\BrainTradeSniperIO-{os.getpid()}-{id(self)}"
        token = "native-integration-token-123456789"
        mailbox = native.NativeTickMailbox()
        server = native._WindowsPipeServer(token, mailbox, pipe_name)
        def wait_until(predicate, timeout=5.0):
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                value = predicate()
                if value:
                    return value
                time.sleep(.01)
            self.fail("Timed out waiting for Windows pipe integration")
        def connect():
            def attempt():
                try:
                    return open(pipe_name, "wb", buffering=0)
                except OSError:
                    return None
            return wait_until(attempt)
        def wire(tms, supplied_token=token):
            return (f"BT2|{supplied_token}|17|XAUUSD|{tms}|"
                    f"{tms*native.EVENT_FACTOR+1}|2000.10|2000.20\n").encode("ascii")
        server.start()
        try:
            with connect() as pipe:
                wait_until(lambda: mailbox.connected)
                now = native.clock_ms()
                payload = wire(now, "wrong-session-token") + wire(now)
                # Force a valid packet to span several independent writes.
                for chunk in (payload[:11],payload[11:37],payload[37:]):
                    pipe.write(chunk)
                packets = wait_until(lambda: mailbox.drain(17, "XAUUSD"))
                self.assertEqual(len(packets), 1, "Wrong token is never admitted")
                self.assertEqual(packets[0]["time_msc"], now)
                self.assertTrue(mailbox.recent(now_ms=native.clock_ms(),maximum_age_ms=2000,
                                               login=17,symbol="XAUUSD"))
            wait_until(lambda: not mailbox.connected)
            with connect() as pipe:
                wait_until(lambda: mailbox.connected)
                self.assertFalse(mailbox.recent(now_ms=native.clock_ms(),maximum_age_ms=2000,
                                                login=17,symbol="XAUUSD"),
                                 "New connection requires new quote evidence")
                fresh = native.clock_ms()
                pipe.write(wire(fresh-5000))
                packets = wait_until(lambda: mailbox.drain(17, "XAUUSD"))
                self.assertEqual(packets[0]["time_msc"],fresh-5000)
                self.assertFalse(mailbox.recent(now_ms=native.clock_ms(),maximum_age_ms=2000,
                                                login=17,symbol="XAUUSD"),
                                 "Delayed native packet cannot disable polling")
                pipe.write(wire(fresh))
                wait_until(lambda: mailbox.drain(17, "XAUUSD"))
                self.assertTrue(mailbox.recent(now_ms=native.clock_ms(),maximum_age_ms=2000,
                                               login=17,symbol="XAUUSD"))
        finally:
            self.assertTrue(server.stop())


if __name__ == "__main__":
    unittest.main()
