"""End-to-end fake-MT5 tests for one-session research-only evidence bundles."""
import importlib.util
from pathlib import Path
from types import SimpleNamespace as NS
import json
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/"gateway"))
spec=importlib.util.spec_from_file_location(
    "demo_evidence_bundle",ROOT/"gateway"/"demo_evidence_bundle.py")
bundle=importlib.util.module_from_spec(spec)
spec.loader.exec_module(bundle)

FROM=1700000000000
TO=FROM+3600000

class FakeDemo:
    ACCOUNT_TRADE_MODE_DEMO=0
    DEAL_TYPE_BUY=0
    DEAL_TYPE_SELL=1
    COPY_TICKS_ALL=-1
    def __init__(self, count=30):
        self.a=NS(login=77,server="Research-Demo",currency="USD",trade_mode=0)
        self.deals=[NS(ticket=i+1000,position_id=i+2000,symbol="XAUUSD",
            entry=0,type=0,volume=.04,price=5000.3,time_msc=FROM+1000,
            profit=0.,commission=-.14,swap=0.,fee=0.) for i in range(count)]
        self.history_reads=0
        self.tick_reads=0
        self.sends=0
        self.switch_second_export=False
    def terminal_info(self): return NS(connected=True)
    def account_info(self):
        if self.switch_second_export and self.history_reads>=2:
            return NS(login=99,server="Different",currency="USD",trade_mode=0)
        return self.a
    def symbol_info(self,symbol):
        return NS(visible=True,trade_tick_size=0.1)
    def history_deals_get(self,start,end):
        self.history_reads+=1
        return self.deals
    def copy_ticks_range(self,symbol,start,end,flags):
        self.tick_reads+=1
        return [{"time_msc":FROM+900,"bid":5000.,"ask":5000.2}]
    def order_send(self,request):
        self.sends+=1
        raise AssertionError("No order may be sent during evidence capture")

class Tests(unittest.TestCase):
    def collect(self,b,**kw):
        return bundle.build_bundle(b,"XAUUSD",FROM,TO,now_ms=TO+1000,**kw)
    def test_integrated_single_session_read_and_offline_reverification(self):
        b=FakeDemo()
        payload=self.collect(b)
        self.assertEqual(payload["cross_evidence_review"]["status"],
            "descriptive_sample_unverified")
        self.assertEqual(b.history_reads,2)
        self.assertEqual(b.tick_reads,30)
        self.assertEqual(b.sends,0)
        self.assertTrue(payload["manifest"]["checksums_not_broker_signature"])
        verify=bundle.verify_bundle(payload)
        self.assertTrue(verify["offline_recalculation_matches"])
        self.assertFalse(verify["broker_authenticated"])
        self.assertFalse(verify["broker_order_sent"])

    def test_independent_checksum_catches_after_capture_edit(self):
        b=FakeDemo()
        payload=self.collect(b)
        payload["deal_export"]["evidence"]["deals"][0]["volume"]=0.05
        with self.assertRaisesRegex(bundle.BundleError,"Checksum mismatch"):
            bundle.verify_bundle(payload)
    def test_rehashed_fake_review_still_fails_recalculation(self):
        b=FakeDemo()
        payload=self.collect(b)
        payload["cross_evidence_review"]["status"]="descriptive_sample_unverified_and_profitable"
        payload["manifest"]["component_sha256"]["cross_evidence_review"]=bundle.digest(
            payload["cross_evidence_review"])
        with self.assertRaisesRegex(bundle.BundleError,"recalculation"):
            bundle.verify_bundle(payload)
    def test_prevents_switch_from_first_to_second_read(self):
        b=FakeDemo()
        b.switch_second_export=True
        with self.assertRaises(bundle.exporter.DemoEvidenceError):
            self.collect(b)
        self.assertEqual(b.sends,0)
    def test_real_account_refuses_any_history_read(self):
        b=FakeDemo()
        b.a.trade_mode=1
        with self.assertRaises(bundle.exporter.DemoEvidenceError):
            self.collect(b)
        self.assertEqual(b.history_reads,0)
    def test_bounded_or_missing_history_does_not_claim_complete_evidence(self):
        b=FakeDemo(count=3)
        result=self.collect(b)
        self.assertEqual(result["cross_evidence_review"]["status"],
            "insufficient_or_sparse_reference")
        self.assertFalse(result["deal_export"]["evidence"]["complete_history"])
        b=FakeDemo()
        b.deals=None
        with self.assertRaises(bundle.exporter.DemoEvidenceError):
            self.collect(b)
    def test_atomic_no_overwrite_and_verification_from_disk(self):
        data=self.collect(FakeDemo())
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/"evidence.json"
            bundle.write_new(path,data)
            self.assertEqual(bundle.load_file(path)["manifest"]["schema"],bundle.SCHEMA)
            self.assertTrue(bundle.verify_bundle(bundle.load_file(path))["checksums_match"])
            with self.assertRaises(bundle.BundleError):
                bundle.write_new(path,data)
            self.assertEqual(len(list(Path(directory).iterdir())),1)
    def test_missing_export_deal_makes_bundle_insufficient_not_equivalent(self):
        b=FakeDemo()
        original=b.history_deals_get
        def change(start,end):
            result=original(start,end)
            return result if b.history_reads==1 else result[:-1]
        b.history_deals_get=change
        out=self.collect(b)
        self.assertEqual(out["cross_evidence_review"]["status"],
            "insufficient_or_sparse_reference")
        self.assertEqual(out["cross_evidence_review"]["missing_price_report_tickets"],[1029])
    def test_unsupported_netting_close_by_fails_closed(self):
        b=FakeDemo()
        b.deals[0].entry=3
        with self.assertRaises(bundle.exporter.DemoEvidenceError):
            self.collect(b)

if __name__=="__main__":
    unittest.main()
