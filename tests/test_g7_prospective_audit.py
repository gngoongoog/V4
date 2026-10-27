"""G7 prospective audit regression tests; no broker or network needed."""
import hashlib
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from g7_prospective_audit import validate

HEX = "a" * 64

def make_study():
    return {"manifest": {
        "protocol": "g7-prospective-v1", "source_commit": "abc",
        "registered_at_ms": 100, "starts_at_ms": 200,
        "frozen_checkpoint_sha256": HEX, "adaptive_checkpoint_sha256": "b"*64,
        "allocation": "sha256-decision-id-parity-v1",
        "minimum_executed_per_arm": 2}, "decisions": [], "outcomes": []}

def add_decision(study, key="trade-1"):
    arm = ("Frozen", "Adaptive")[int(hashlib.sha256(key.encode()).hexdigest(), 16) % 2]
    row = {"decision_id": key, "assigned_arm": arm, "decided_at_ms": 300,
           "checkpoint_sha256": study["manifest"]["frozen_checkpoint_sha256" if arm == "Frozen" else "adaptive_checkpoint_sha256"],
           "account": "demo:17", "symbol": "XAUUSD", "mode": "sniper",
           "snapshot_sha256": HEX}
    study["decisions"].append(row)
    return row

def booked(uid, **overrides):
    row = {"decision_id": uid, "kind": "mt5_demo_execution",
           "broker_account": "demo:17", "symbol": "XAUUSD", "trade_mode": "DEMO",
           "position_id": "P1", "closed_at_ms": 400,
           "gross": 10.0, "commission": -1.0, "swap": 0.0, "fee": 0.0,
           "risk_money": 10.0, "net": 9.0, "net_r": 0.9}
    row.update(overrides)
    return row

class StudyTests(unittest.TestCase):
    def test_empty_is_not_started(self):
        self.assertEqual(validate(make_study())["status"], "not_started")

    def test_realish_rows_still_inconclusive(self):
        s = make_study()
        add_decision(s)
        s["outcomes"].append(booked("trade-1"))
        result = validate(s)
        self.assertEqual(result["status"], "inconclusive")
        self.assertFalse(result["release_authorized"])

    def test_reject_modified_fee(self):
        s = make_study()
        add_decision(s)
        s["outcomes"].append(booked("trade-1", commission=-7))
        with self.assertRaisesRegex(ValueError, "fee reconciliation"):
            validate(s)

    def test_reject_counterfactual_booked_as_execution(self):
        s = make_study()
        add_decision(s)
        s["outcomes"].append(booked("trade-1", trade_mode="REAL"))
        with self.assertRaisesRegex(ValueError, "Demo"):
            validate(s)

    def test_reject_nonprospective_and_wrong_allocation(self):
        s = make_study()
        d = add_decision(s)
        d["decided_at_ms"] = 199
        with self.assertRaisesRegex(ValueError, "nonprospective"):
            validate(s)
        d["decided_at_ms"] = 300
        d["assigned_arm"] = "Frozen" if d["assigned_arm"] == "Adaptive" else "Adaptive"
        with self.assertRaisesRegex(ValueError, "allocation"):
            validate(s)

    def test_no_orphan_duplicate_or_nan(self):
        s = make_study()
        add_decision(s)
        s["outcomes"].append(booked("trade-1", net=float("nan")))
        with self.assertRaisesRegex(ValueError, "nonfinite"):
            validate(s)
        s["outcomes"] = [booked("trade-1")] * 2
        with self.assertRaisesRegex(ValueError, "orphan/duplicate"):
            validate(s)

    def test_quote_only_does_not_count_as_booked(self):
        s = make_study()
        add_decision(s)
        s["outcomes"].append({"decision_id": "trade-1", "kind": "counterfactual_signal"})
        result = validate(s)
        self.assertEqual(result["counts"].get("booked_Frozen", 0), 0)
        self.assertEqual(result["counts"].get("booked_Adaptive", 0), 0)

if __name__ == "__main__":
    unittest.main()
