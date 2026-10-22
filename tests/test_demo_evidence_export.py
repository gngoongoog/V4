"""Unit tests for the strictly read-only and explicit MT5 Demo exporter."""
import importlib.util
from pathlib import Path
from types import SimpleNamespace as NS
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("demo_evidence_export", ROOT/"gateway"/"demo_evidence_export.py")
exporter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(exporter)

FROM = 1_700_000_000_000
TO = FROM + 60_000


def deal(ticket=101, position_id=77, entry=0, volume=0.04):
    return NS(ticket=ticket, position_id=position_id, entry=entry,
              symbol="XAUUSD", volume=volume, price=5000.2,
              time_msc=FROM + 1000 + ticket, profit=2.0,
              commission=-0.14, swap=0.0, fee=0.0)


class FakeDemo:
    ACCOUNT_TRADE_MODE_DEMO=0

    def __init__(self):
        self.a = NS(login=77, server="Demo-Server", currency="USD", trade_mode=0)
        self.deals = [deal(1, entry=0), deal(2, entry=1)]
        self.sent = 0
        self.history_calls = []
        self.switch_account = False

    def terminal_info(self):
        return NS(connected=True)

    def account_info(self):
        if self.switch_account and self.history_calls:
            return NS(login=99, server="Other-Demo", currency="USD", trade_mode=0)
        return self.a

    def symbol_info(self, symbol):
        return NS(name=symbol)

    def history_deals_get(self, since, until):
        self.history_calls.append((since, until))
        return self.deals

    def order_send(self, request):
        self.sent += 1
        raise AssertionError("Exporter must never send MT5 orders")


class DemoReadTests(unittest.TestCase):
    def capture(self, broker, **kwargs):
        return exporter.capture(broker, "XAUUSD", FROM, TO, now_ms=TO+1000, **kwargs)

    def test_read_only_demo_history_and_no_claim_to_verified_parity(self):
        broker=FakeDemo()
        out=self.capture(broker, mapping={"77":"A"})
        self.assertEqual(out["evidence"]["source"], "caller_supplied_mt5_demo_deals")
        self.assertEqual(out["evidence"]["account"], "77")
        self.assertEqual(out["evidence"]["server"], "Demo-Server")
        self.assertFalse(out["evidence"]["complete_history"])
        self.assertEqual(len(out["evidence"]["deals"]),2)
        self.assertEqual(out["evidence"]["deals"][0]["intent_id"],"A")
        self.assertTrue(out["export_metadata"]["account_identity_checked_before_after_read"])
        self.assertEqual(broker.sent,0)
        self.assertEqual(len(broker.history_calls),1)

    def test_unmapped_position_stays_unlinked_not_guessed_by_volume(self):
        out=self.capture(FakeDemo())
        self.assertIsNone(out["evidence"]["deals"][0]["intent_id"])

    def test_refuses_real_and_contest_accounts(self):
        broker=FakeDemo()
        broker.a.trade_mode=1
        with self.assertRaises(exporter.DemoEvidenceError):
            self.capture(broker)
        self.assertEqual(len(broker.history_calls),0)

    def test_account_switch_invalidates_all_history(self):
        broker=FakeDemo()
        broker.switch_account=True
        with self.assertRaisesRegex(exporter.DemoEvidenceError,"switched"):
            self.capture(broker)

    def test_none_history_is_not_an_empty_history_proof(self):
        broker=FakeDemo()
        broker.deals=None
        with self.assertRaisesRegex(exporter.DemoEvidenceError,"unavailable"):
            self.capture(broker)
        broker.deals=[]
        out=self.capture(broker)
        self.assertEqual(out["evidence"]["deals"],[])
        self.assertFalse(out["evidence"]["complete_history"])

    def test_duplicate_ticket_or_broker_close_by_is_rejected(self):
        broker=FakeDemo()
        broker.deals=[deal(1),deal(1)]
        with self.assertRaisesRegex(exporter.DemoEvidenceError,"Duplicate"):
            self.capture(broker)
        broker.deals=[deal(1,entry=3)]
        with self.assertRaisesRegex(exporter.DemoEvidenceError,"Unsupported"):
            self.capture(broker)

    def test_future_or_outside_window_event_rejected(self):
        broker=FakeDemo()
        d=deal(1)
        d.time_msc=TO+1
        broker.deals=[d]
        with self.assertRaisesRegex(exporter.DemoEvidenceError,"outside"):
            self.capture(broker)
        with self.assertRaises(exporter.DemoEvidenceError):
            exporter.capture(broker, "XAUUSD", TO, FROM, now_ms=TO+1000)

    def test_validates_operator_mapping_strictly(self):
        self.assertEqual(exporter.validate_mapping({"77":"A"}),{77:"A"})
        for bad in ({"-1":"A"},{"nonnumeric":"A"},{"77":"A bad intent"}, {"77":None}):
            with self.subTest(bad=bad):
                with self.assertRaises(exporter.DemoEvidenceError):
                    exporter.validate_mapping(bad)

    def test_other_symbol_history_is_not_silently_mislabeled_as_gold(self):
        broker=FakeDemo()
        other=deal(3)
        other.symbol="EURUSD"
        broker.deals=[deal(1),other]
        out=self.capture(broker)
        self.assertEqual(len(out["evidence"]["deals"]),1)
        self.assertEqual(out["export_metadata"]["excluded_other_symbol_deals"],1)


if __name__ == "__main__":
    unittest.main()
