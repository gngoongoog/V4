"""Pure offline tests: two evidence JSON exports, never broker calls."""
import importlib.util
from pathlib import Path
import unittest

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location(
    "demo_cross_evidence_review", ROOT/"gateway"/"demo_cross_evidence_review.py")
review=importlib.util.module_from_spec(spec)
spec.loader.exec_module(review)
BASE=1700000000000
END=BASE+60000

def fixtures(count=30,has_ticks=True):
    deal_rows=[]
    price_rows=[]
    for i in range(count):
        ticket=1000+i
        deal={"ticket":ticket,"position_id":800+i,"intent_id":str(i),
            "direction":"in","volume":0.04,"price":5000.3,
            "time_msc":BASE+1000*(i+1),"profit":1.,"commission":-0.14,
            "swap":0.,"fee":0.}
        price={"deal_ticket":ticket,"position_id":deal["position_id"],
            "deal_time_ms":deal["time_msc"],"deal_entry_code":0,
            "broker_deal_side":"buy","lots":0.04,"execution_price":5000.3,
            "fees_and_commission_booked_on_supplied_deal":-0.14,
            "broker_reported_profit":1.,"broker_reported_swap":0.,
            "reference_quote_price":5000.2 if has_ticks else None,
            "reference_quote_age_ms":100 if has_ticks else None,
            "historical_spread_ticks":2. if has_ticks else None,
            "indicative_adverse_deviation_ticks":1. if has_ticks else None}
        deal_rows.append(deal)
        price_rows.append(price)
    deal_export={"evidence":{"source":"caller_supplied_mt5_demo_deals",
        "account":"77","server":"Demo","symbol":"XAUUSD","currency":"USD",
        "demo":True,"complete_history":False,"deals":deal_rows},
        "export_metadata":{"kind":"local_demo_read_only_unattested",
            "window_from_ms":BASE,"window_to_ms_exclusive":END}}
    prices={"source":"locally_observed_mt5_demo_unattested",
        "policy":"read_only_demo_historical_price_audit_v1",
        "account":"77","server":"Demo","symbol":"XAUUSD","currency":"USD",
        "demo":True,"complete_history":False,"window_start_utc_ms":BASE,
        "window_end_utc_ms_exclusive":END,
        "tick_size":0.1,"requested_max_quote_age_ms":1500,
        "deal_count":count,"deals_with_valid_prior_quote":count if has_ticks else 0,
        "deals_missing_reference":0 if has_ticks else count,
        "mean_indicative_adverse_deviation_ticks":1. if has_ticks else None,
        "rows":price_rows}
    return deal_export,prices

class Tests(unittest.TestCase):
    def test_matching_sample_is_never_broker_attestation(self):
        a,b=fixtures()
        result=review.compare(a,b)
        self.assertEqual(result["status"],"descriptive_sample_unverified")
        self.assertEqual(result["matched_quote_samples"],30)
        self.assertAlmostEqual(result["adverse_deviation_ticks"]["p95"],1.)
        self.assertEqual(result["mt5_connection_performed"],False)
        self.assertEqual(result["broker_order_sent"],False)
        self.assertEqual(result["profitability_or_execution_quality_certified"],False)

    def test_real_or_changed_account_is_rejected(self):
        a,b=fixtures()
        b["demo"]=False
        with self.assertRaises(review.EvidenceError):
            review.compare(a,b)
        a,b=fixtures()
        b["account"]="78"
        with self.assertRaises(review.EvidenceError):
            review.compare(a,b)
        a,b=fixtures()
        b["symbol"]="EURUSD"
        with self.assertRaises(review.EvidenceError):
            review.compare(a,b)

    def test_mismatch_of_volume_price_commission_position_or_time(self):
        for field,value in (("lots",0.05),("execution_price",5001.3),
                            ("fees_and_commission_booked_on_supplied_deal",0.14),
                            ("position_id",9999),("deal_time_ms",BASE+2000)):
            with self.subTest(field=field):
                a,b=fixtures()
                b["rows"][0][field]=value
                r=review.compare(a,b)
                self.assertEqual(r["status"],"contradictory_exported_records")
                self.assertIn(1000,r["contradictory_tickets_or_headers"])

    def test_future_or_impossibly_old_quote_evidence_is_contradictory(self):
        a,b=fixtures()
        b["rows"][0]["reference_quote_age_ms"]=-50
        self.assertEqual(review.compare(a,b)["status"],"contradictory_exported_records")
        a,b=fixtures()
        b["rows"][0]["reference_quote_age_ms"]=5000
        self.assertEqual(review.compare(a,b)["status"],"contradictory_exported_records")

    def test_unmatched_or_duplicate_tickets_cannot_look_fully_reconciled(self):
        a,b=fixtures()
        b["rows"][0]["deal_ticket"]=123456
        r=review.compare(a,b)
        self.assertEqual(r["status"],"contradictory_exported_records")
        self.assertIn(123456,r["unexpected_price_report_tickets"])
        self.assertIn(1000,r["missing_price_report_tickets"])
        a,b=fixtures()
        b["rows"][1]["deal_ticket"]=1000
        with self.assertRaises(review.EvidenceError):
            review.compare(a,b)

    def test_missing_quotes_are_unknown_not_zero_slippage(self):
        a,b=fixtures(has_ticks=False)
        r=review.compare(a,b)
        self.assertEqual(r["status"],"insufficient_or_sparse_reference")
        self.assertIsNone(r["adverse_deviation_ticks"]["p99"])
        self.assertEqual(len(r["lacking_prior_quote_tickets"]),30)

    def test_small_sample_is_not_sufficient(self):
        a,b=fixtures(4)
        r=review.compare(a,b)
        self.assertEqual(r["status"],"insufficient_or_sparse_reference")
        self.assertEqual(r["matched_quote_samples"],4)

    def test_summary_tampering_is_detected(self):
        a,b=fixtures()
        b["mean_indicative_adverse_deviation_ticks"]=8.0
        r=review.compare(a,b)
        self.assertIn("quote_mean_header",r["contradictory_tickets_or_headers"])
        a,b=fixtures()
        b["deal_count"]=999
        with self.assertRaises(review.EvidenceError):
            review.compare(a,b)

    def test_false_full_history_claim_is_refused(self):
        a,b=fixtures()
        a["evidence"]["complete_history"]=True
        with self.assertRaises(review.EvidenceError):
            review.compare(a,b)

if __name__=="__main__":
    unittest.main()
