import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).parents[1]
sys.path.insert(0, str(ROOT / "gateway"))

import sniper_native as native


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


gateway = load("native_tick_gateway", ROOT / "gateway" / "mt5_gateway.py")
packaging = load("native_tick_packaging", ROOT / "scripts" / "package-portable.py")


class NativeSniperTests(unittest.TestCase):
    def test_authenticated_bt2_packet_and_bt1_compatibility(self):
        token = "session-token-0123456789"
        event_id = 2_000 * native.EVENT_FACTOR + 7
        packet = native.parse_packet(
            f"BT2|{token}|77|XAUUSD|2000|{event_id}|2500.10|2500.30\n",
            token,
            received_ms=2004,
        )
        self.assertEqual(packet["login"], 77)
        self.assertEqual(packet["symbol"], "XAUUSD")
        self.assertEqual(packet["time_msc"], 2000)
        self.assertEqual(packet["event_id"], event_id)
        self.assertEqual(packet["received_ms"], 2004)
        legacy = native.parse_packet(
            f"BT1|{token}|77|XAUUSD|2000|2500.10|2500.30", token, received_ms=2004
        )
        self.assertEqual(legacy["event_id"], 2_000 * native.EVENT_FACTOR)
        with self.assertRaisesRegex(ValueError, "token"):
            native.parse_packet(f"BT2|wrong|77|XAUUSD|2000|{event_id}|2500.1|2500.3", token)
        with self.assertRaisesRegex(ValueError, "event ID"):
            native.parse_packet(f"BT2|{token}|77|XAUUSD|2000|123|2500.1|2500.3", token)
        with self.assertRaisesRegex(ValueError, "quote"):
            native.parse_packet(f"BT2|{token}|77|XAUUSD|2000|{event_id}|2501|2500", token)

    def test_native_pipe_bytearray_packet_from_partition_is_valid(self):
        # Regression: the Windows pipe worker partitions a bytearray buffer.
        # Prior to this fix the parser tried bytearray.encode() and crashed
        # the braintrade-sniper-pipe thread, leaving the EA in WAITING.
        token = "session-token-0123456789"
        event_id = 2_000 * native.EVENT_FACTOR + 7
        wire = f"BT2|{token}|77|XAUUSDr|2000|{event_id}|2500.10|2500.30\n"
        pending = bytearray(wire.encode("ascii"))
        line, separator, remainder = pending.partition(b"\n")
        self.assertIsInstance(line, bytearray)
        self.assertEqual(separator, bytearray(b"\n"))
        self.assertEqual(remainder, bytearray())
        parsed = native.parse_packet(line, token, received_ms=2004)
        self.assertEqual(parsed["login"], 77)
        self.assertEqual(parsed["symbol"], "XAUUSDr")
        self.assertEqual(parsed["event_id"], event_id)
        self.assertEqual(native.parse_packet(bytes(line), token, received_ms=2004), parsed)
        source = (ROOT / "gateway" / "sniper_native.py").read_text(encoding="utf-8")
        self.assertIn("parse_packet(bytes(line), self.token)", source)
        with self.assertRaisesRegex(ValueError, "token"):
            native.parse_packet(line, "wrong-session-token")

    def test_session_token_uses_selected_terminal_data_folder(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = native.write_session_token(tmp, "temporary-secret-token")
            self.assertEqual(path, Path(tmp) / "MQL5/Files/BrainTrade/sniper.token")
            self.assertEqual(path.read_text("ascii"), "temporary-secret-token")

    def test_mailbox_keeps_tick_order_and_records_overflow(self):
        box = native.NativeTickMailbox(limit=2)
        for index, tms in enumerate((1000, 1001, 1002), start=1):
            box.push(dict(login=7, symbol="XAUUSD", time_msc=tms,
                          event_id=tms*native.EVENT_FACTOR+index,
                          bid=1.0, ask=1.1, received_ms=tms))
        self.assertEqual(box.dropped, 1)
        drained = box.drain(7, "XAUUSD", maximum=8)
        self.assertEqual([item["time_msc"] for item in drained], [1001, 1002])

    def test_native_overlay_preserves_reconciled_state_and_rejects_wrong_identity(self):
        snapshot = {
            "connected": True,
            "login": 77,
            "server": "Demo",
            "symbol": "XAUUSD",
            "bid": 2500.0,
            "ask": 2500.2,
            "quote_time_ms": 1000,
            "source_quote_time_ms": 1000,
            "positions": [{"ticket": 5}],
            "market": {"m1": [1]},
        }
        packet = dict(login=77, symbol="XAUUSD", time_msc=2000,
                      event_id=2000*native.EVENT_FACTOR+1,bid=2500.1, ask=2500.3)
        updated, changed, signature = gateway.apply_native_tick(snapshot, packet, now_ms=2004)
        self.assertTrue(changed)
        self.assertEqual(signature, (2000, 2500.1, 2500.3))
        self.assertEqual(updated["tick_age_ms"], 4)
        self.assertEqual(updated["positions"], snapshot["positions"])
        self.assertIs(updated["market"], snapshot["market"])
        bad = dict(packet, login=88)
        same, changed, signature = gateway.apply_native_tick(snapshot, bad, now_ms=2004)
        self.assertIs(same, snapshot)
        self.assertFalse(changed)
        self.assertIsNone(signature)
        stale = dict(packet, time_msc=900, event_id=900*native.EVENT_FACTOR+1)
        self.assertFalse(gateway.apply_native_tick(snapshot, stale, now_ms=2004)[1])

    def test_same_millisecond_quote_changes_get_distinct_core_event_times(self):
        snapshot = {
            "connected": True, "login": 77, "symbol": "XAUUSD",
            "bid": 2500.0, "ask": 2500.2,
            "quote_time_ms": 2000, "source_quote_time_ms": 2000,
        }
        first = dict(login=77,symbol="XAUUSD",time_msc=2000,
                     event_id=2000*native.EVENT_FACTOR+1,bid=2500.1,ask=2500.3)
        one, changed, _ = gateway.apply_native_tick(snapshot,first,now_ms=2001)
        self.assertTrue(changed)
        self.assertEqual(one["source_quote_time_ms"],2000)
        self.assertEqual(one["quote_time_ms"],2001)
        second = dict(first,event_id=2000*native.EVENT_FACTOR+2,bid=2500.2,ask=2500.4)
        two, changed, _ = gateway.apply_native_tick(one,second,now_ms=2001)
        self.assertTrue(changed)
        self.assertEqual(two["source_quote_time_ms"],2000)
        self.assertEqual(two["quote_time_ms"],2002)
        duplicate, changed, _ = gateway.apply_native_tick(two,second,now_ms=2001)
        self.assertIs(duplicate,two)
        self.assertFalse(changed)

    def test_full_snapshot_does_not_rewind_synthetic_sniper_event_clock(self):
        previous = {
            "connected":True,"bid":2500.2,"ask":2500.4,
            "quote_time_ms":2002,"source_quote_time_ms":2000,
            "native_event_id":2000*native.EVENT_FACTOR+2,
        }
        full = {
            "connected":True,"bid":2500.2,"ask":2500.4,
            "quote_time_ms":2000,"source_quote_time_ms":2000,
            "native_event_id":None,
        }
        merged,changed=gateway.advance_quote_clock(previous,full)
        self.assertFalse(changed)
        self.assertEqual(merged["quote_time_ms"],2002)
        self.assertEqual(merged["native_event_id"],previous["native_event_id"])

    def test_native_wakeup_precedes_fallback_poll_contract(self):
        source = (ROOT / "gateway" / "mt5_gateway.py").read_text(encoding="utf-8")
        self.assertIn("native.drain", source)
        self.assertIn("native.wait(remaining)", source)
        self.assertIn("not native.recent", source)
        self.assertIn("refresh_fast_tick", source)
        self.assertIn("advance_quote_clock", source)
        self.assertLess(source.index("packets=native.drain"), source.index("full_due=latest_snapshot"))
        self.assertLess(source.index("native.wait(remaining)"), source.index("time.sleep(remaining)"))

    def test_mql5_bridge_recovers_cached_quote_ticks_and_never_trades(self):
        source = (ROOT / "mt5" / "BrainTradeSniperBridge.mq5").read_text(encoding="utf-8")
        self.assertIn("void OnTick()", source)
        self.assertIn("CopyTicks(_Symbol", source)
        self.assertIn("COPY_TICKS_ALL", source)
        self.assertIn("TICK_FLAG_BID|TICK_FLAG_ASK", source)
        self.assertIn("BT2|", source)
        self.assertIn("FileOpen(InpPipeName", source)
        self.assertIn("FileWriteString", source)
        self.assertEqual(source.count("FileFlush(g_pipe)"),1)
        self.assertIn("BrainTrade\\\\sniper.token", source)
        self.assertNotIn("OrderSend", source)
        self.assertNotIn("CTrade", source)
        self.assertNotIn("trade.Buy", source)
        self.assertNotIn("trade.Sell", source)

    def test_windows_package_places_native_bridge_next_to_runtime(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "source"
            output = Path(tmp) / "package"
            (root / "gateway").mkdir(parents=True)
            (root / "mt5").mkdir(parents=True)
            for name in packaging.GATEWAY_FILES:
                (root / "gateway" / name).write_text(name, encoding="utf-8")
            (root / "mt5" / packaging.NATIVE_EA).write_text("EA", encoding="utf-8")
            previous = packaging.ROOT
            packaging.ROOT = root
            try:
                packaging.copy_runtime_assets(output)
            finally:
                packaging.ROOT = previous
            for name in packaging.GATEWAY_FILES:
                self.assertTrue((output / "runtime/gateway" / name).is_file())
            self.assertEqual((output / "MT5/Experts" / packaging.NATIVE_EA).read_text(), "EA")


if __name__ == "__main__":
    unittest.main()
