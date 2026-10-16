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
    def test_authenticated_packet_parsing(self):
        token = "session-token-0123456789"
        packet = native.parse_packet(
            f"BT1|{token}|77|XAUUSD|2000|2500.10|2500.30\n",
            token,
            received_ms=2004,
        )
        self.assertEqual(packet["login"], 77)
        self.assertEqual(packet["symbol"], "XAUUSD")
        self.assertEqual(packet["time_msc"], 2000)
        self.assertEqual(packet["received_ms"], 2004)
        with self.assertRaisesRegex(ValueError, "token"):
            native.parse_packet("BT1|wrong|77|XAUUSD|2000|2500.1|2500.3", token)
        with self.assertRaisesRegex(ValueError, "quote"):
            native.parse_packet(f"BT1|{token}|77|XAUUSD|2000|2501|2500", token)

    def test_session_token_uses_selected_terminal_data_folder(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = native.write_session_token(tmp, "temporary-secret-token")
            self.assertEqual(path, Path(tmp) / "MQL5/Files/BrainTrade/sniper.token")
            self.assertEqual(path.read_text("ascii"), "temporary-secret-token")

    def test_mailbox_keeps_tick_order_and_records_overflow(self):
        box = native.NativeTickMailbox(limit=2)
        for tms in (1000, 1001, 1002):
            box.push(dict(login=7, symbol="XAUUSD", time_msc=tms, bid=1.0, ask=1.1, received_ms=tms))
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
            "positions": [{"ticket": 5}],
            "market": {"m1": [1]},
        }
        packet = dict(login=77, symbol="XAUUSD", time_msc=2000, bid=2500.1, ask=2500.3)
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
        stale = dict(packet, time_msc=900)
        self.assertFalse(gateway.apply_native_tick(snapshot, stale, now_ms=2004)[1])

    def test_native_wakeup_precedes_fallback_poll_contract(self):
        source = (ROOT / "gateway" / "mt5_gateway.py").read_text(encoding="utf-8")
        self.assertIn("native.drain", source)
        self.assertIn("native.wait(remaining)", source)
        self.assertIn("not native.recent", source)
        self.assertIn("refresh_fast_tick", source)
        self.assertLess(source.index("packets=native.drain"), source.index("full_due=latest_snapshot"))
        self.assertLess(source.index("native.wait(remaining)"), source.index("time.sleep(remaining)"))

    def test_mql5_bridge_is_tick_transport_only(self):
        source = (ROOT / "mt5" / "BrainTradeSniperBridge.mq5").read_text(encoding="utf-8")
        self.assertIn("void OnTick()", source)
        self.assertIn("FileOpen(InpPipeName", source)
        self.assertIn("FileWriteString", source)
        self.assertIn("FileFlush(g_pipe)", source)
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
