"""Authenticated EA heartbeat and trade-notice parsing, no MT5 or broker calls."""
from pathlib import Path
from unittest import TestCase, main
import sys

sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"gateway"))
from sniper_native import (
    parse_control_packet, parse_packet, NativeTickMailbox, PROTOCOL
)


TOKEN="known-example-token-123456789"


class NativeControlProtocolTests(TestCase):
    def test_heartbeat_is_authenticated_but_never_a_quote(self):
        event=parse_control_packet(f"BTH1|{TOKEN}|17|XAUUSDm|1".encode(),
                                  TOKEN,received_ms=6000)
        self.assertEqual(event,{"kind":"heartbeat","login":17,
                         "symbol":"XAUUSDm","sequence":1,"received_ms":6000})
        mailbox=NativeTickMailbox(limit=2)
        mailbox.set_connected(True)
        mailbox.note_control(event)
        self.assertEqual(mailbox.control_heartbeats,1)
        self.assertFalse(mailbox.recent(now_ms=6000,login=17,symbol="XAUUSDm"))
        self.assertEqual(mailbox.drain(17,"XAUUSDm"),[])
        self.assertFalse(mailbox.consume_trade_notice(17,"XAUUSDm"))

    def test_trade_notice_requests_once_only_matching_account_and_symbol(self):
        box=NativeTickMailbox(limit=1)
        box.set_connected(True)
        box.note_control(parse_control_packet(
            f"BTE1|{TOKEN}|17|XAUUSDm|2",TOKEN,received_ms=6002))
        self.assertEqual(box.trade_wakes,1)
        self.assertFalse(box.recent(now_ms=6002,login=17,symbol="XAUUSDm"))
        self.assertFalse(box.consume_trade_notice(18,"XAUUSDm"))
        self.assertFalse(box.consume_trade_notice(17,"XAUUSDm"),
                         "Foreign notices must not be held for another account")
        box.note_control(parse_control_packet(
            f"BTE1|{TOKEN}|17|XAUUSDm|3",TOKEN,received_ms=6003))
        self.assertTrue(box.consume_trade_notice(17,"XAUUSDm"))
        self.assertFalse(box.consume_trade_notice(17,"XAUUSDm"))

    def test_disconnect_clears_trade_wakes_and_control_liveness(self):
        box=NativeTickMailbox()
        box.set_connected(True)
        box.note_control(parse_control_packet(
            f"BTE1|{TOKEN}|17|XAUUSDm|1",TOKEN,received_ms=6000))
        box.set_connected(False)
        self.assertFalse(box.consume_trade_notice(17,"XAUUSDm"))
        self.assertEqual(box.last_control_received_ms,0)
        box.set_connected(True)
        self.assertFalse(box.consume_trade_notice(17,"XAUUSDm"))

    def test_forged_control_frames_cannot_trigger_reconciliation(self):
        invalid=[
            f"BTE1|wrong-token|17|XAUUSDm|1",
            f"BTE1|{TOKEN}|0|XAUUSDm|1",
            f"BTH1|{TOKEN}|17|XAUUSDm|0",
            f"BTE1|{TOKEN}|17|XAUUSDm|-1",
            f"BTE1|{TOKEN}|17|XAUUSDm|4294967296",
            f"BTH1|{TOKEN}|abc|XAUUSDm|1",
            f"BTE1|{TOKEN}|17||1",
            f"BTE1|{TOKEN}|17|XAUUSDm",
            f"BTE1|{TOKEN}|17|XAUUSDm|1|extra",
            f"BT2|{TOKEN}|17|XAUUSDm|1",
            "BTH1|"+"a"*600,
        ]
        for raw in invalid:
            with self.subTest(raw=raw[:24]),self.assertRaises(ValueError):
                parse_control_packet(raw,TOKEN)
        self.assertRaises(ValueError,parse_packet,
                          f"BTE1|{TOKEN}|17|XAUUSDm|1",TOKEN)

    def test_control_frames_do_not_mutate_tick_event_lineage(self):
        box=NativeTickMailbox()
        box.set_connected(True)
        box.push({"login":17,"symbol":"XAUUSDm","received_ms":6000,
                  "time_msc":6000,"bid":2000.,"ask":2000.2,
                  "event_id":6000000001})
        before=(box.last_quote_time_ms,box.last_received_ms,box.last_login)
        box.note_control(parse_control_packet(
            f"BTH1|{TOKEN}|17|XAUUSDm|10",TOKEN,received_ms=6001))
        self.assertEqual(before,(box.last_quote_time_ms,box.last_received_ms,box.last_login))
        self.assertEqual(len(box.drain(17,"XAUUSDm")),1)

if __name__=="__main__":
    main()
