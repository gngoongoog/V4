"""P0-E: account-scoped native quotes and clean MT5 reconnect lifecycle.

Pure simulation only; no MetaTrader connection and no broker order sends.
"""
from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path
from unittest import TestCase, main
from unittest.mock import patch
import sys
import tempfile

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'gateway'))
import mt5_gateway as gateway
import sniper_native as native


class P0ENativeLifecycle(TestCase):
    def packet(self, login=17, symbol='XAUUSD', ms=1000):
        return dict(login=login,symbol=symbol,time_msc=ms,
                    event_id=ms*native.EVENT_FACTOR+1,bid=2500.,ask=2500.2,
                    received_ms=ms)

    def test_mailbox_heartbeat_is_for_the_selected_login_and_symbol(self):
        box=native.NativeTickMailbox(limit=3)
        box.set_connected(True)
        box.push(self.packet(login=17,symbol='XAUUSD'))
        self.assertTrue(box.recent(now_ms=1005,login=17,symbol='XAUUSD'))
        self.assertFalse(box.recent(now_ms=1005,login=18,symbol='XAUUSD'))
        self.assertFalse(box.recent(now_ms=1005,login=17,symbol='EURUSD'))
        self.assertFalse(box.recent(now_ms=2000,login=17,symbol='XAUUSD'))

    def test_native_disconnect_revokes_freshness_and_pending_packets(self):
        box=native.NativeTickMailbox(limit=2)
        box.set_connected(True)
        box.push(self.packet())
        box.reset()
        self.assertFalse(box.connected)
        self.assertFalse(box.recent(now_ms=1000,login=17,symbol='XAUUSD'))
        self.assertEqual(box.drain(17,'XAUUSD'),[])
        self.assertEqual(box.last_received_ms,0)

    def test_clean_native_bridge_close_revokes_token_and_clears_queue(self):
        with tempfile.TemporaryDirectory() as tmp:
            bridge=native.NativeTickBridge()
            bridge.token='secret-identifier'
            bridge.data_path=Path(tmp)
            bridge.token_path=native.write_session_token(tmp,bridge.token)
            class FakeServer:
                def stop(self):return True
            bridge.server=FakeServer()
            bridge.mailbox.set_connected(True)
            bridge.mailbox.push(self.packet())
            self.assertTrue(bridge.close())
            self.assertFalse(native.session_token_path(tmp).exists())
            self.assertEqual(bridge.drain(17,'XAUUSD'),[])
            self.assertFalse(bridge.recent(now_ms=1000,login=17,symbol='XAUUSD'))

    def test_stuck_native_pipe_does_not_suppress_polling_or_expose_old_ticks(self):
        bridge=native.NativeTickBridge()
        class StuckServer:
            running=True
            def stop(self):return False
        bridge.server=StuckServer()
        bridge.mailbox.set_connected(True)
        bridge.mailbox.push(self.packet())
        self.assertFalse(bridge.close())
        self.assertTrue(bridge.blocked)
        self.assertFalse(bridge.recent(now_ms=1000,login=17,symbol='XAUUSD'))
        self.assertEqual(bridge.drain(17,'XAUUSD'),[])

    def test_quote_clock_cannot_borrow_a_previous_account_mode_or_symbol(self):
        previous=dict(connected=True,login=17,server='S',currency='USD',
                      account_trade_mode=0,symbol='XAUUSD',bid=2500.,ask=2500.2,
                      source_quote_time_ms=1200,quote_time_ms=1600,
                      native_event_id=1200*native.EVENT_FACTOR+7)
        for changed in ('account_trade_mode','server','login','symbol'):
            with self.subTest(changed=changed):
                current=dict(previous,source_quote_time_ms=1225,
                             quote_time_ms=1225,native_event_id=None,bid=2500.1)
                current[changed]={'account_trade_mode':2,'server':'Other',
                                  'login':18,'symbol':'XAUUSDm'}[changed]
                result,has_quote=gateway.advance_quote_clock(previous,current)
                self.assertEqual(result['quote_time_ms'],1225)
                self.assertTrue(has_quote)
                self.assertIsNone(result.get('native_event_id'))

    def test_market_and_dom_caches_are_mode_scoped(self):
        now=datetime.now(timezone.utc)
        cache=gateway.MarketCache()
        with patch.object(gateway,'collect_market',return_value={'m1':[]}) as collected:
            cache.read(None,(17,'S','XAUUSD','USD',0),now)
            cache.read(None,(17,'S','XAUUSD','USD',0),now)
            cache.read(None,(17,'S','XAUUSD','USD',2),now)
            self.assertEqual(collected.call_count,2)
        gateway_source=(ROOT/'gateway'/'mt5_gateway.py').read_text(encoding='utf-8')
        self.assertIn('account.trade_mode)',gateway_source)
        self.assertIn('native.close()',gateway_source)
        self.assertIn('native.recent(login=',gateway_source)


if __name__=='__main__':main()
