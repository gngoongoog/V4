"""Adversarial G6-G local admission-clock lineage tests; fake SQLite only."""
from __future__ import annotations
import hashlib
import importlib.util
import json
from pathlib import Path
import sqlite3
import sys
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/"scripts"))
import audit_v25_g6g_broker_admission_clock as g6g
spec=importlib.util.spec_from_file_location(
    "g6g_g6d_fixture",ROOT/"tests"/"test_v25_g6d_causal_learning_evidence.py")
fixture=importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)


class G6GAdmittedLearning(unittest.TestCase):
    def setUp(self):
        self.fx=fixture.TestCausalMemberLearning()
        self.fx.setUp()
        self.db=self.fx.db
        self.db.row_factory=sqlite3.Row
        self.db.executescript("""
        CREATE TABLE coordinator_broker_admissions(
            source TEXT NOT NULL,position_id INTEGER NOT NULL,context TEXT NOT NULL,
            brain TEXT NOT NULL,closed_ms INTEGER NOT NULL,admitted_ms INTEGER NOT NULL,
            payload_sha256 TEXT NOT NULL,protocol TEXT NOT NULL,
            PRIMARY KEY(source,position_id));
        """)
        self.observed=(fixture.FUTURE+60)*1000+500
        self.admitted=self.observed-100
        self.closed=self.admitted-1000

    def tearDown(self):
        self.fx.tearDown()

    def add(self,*,with_admission=True,admitted=None):
        self.fx.receipt(self.closed)
        # Older G6-D arithmetic-only fixture omits three source fields.
        # Real Rust G6-F/G6-G receipts contain all three and MUST match the
        # canonical source tuple; keep the production audit strict.
        row=self.db.execute(
            "SELECT source,position_id,payload FROM coordinator_broker_samples").fetchone()
        expanded=json.loads(row["payload"])
        expanded.update({"trading_mode":"expert","timeframe":"M1","report_id":""})
        self.db.execute(
            "UPDATE coordinator_broker_samples SET payload=? "
            "WHERE source=? AND position_id=?",
            (json.dumps(expanded),row["source"],row["position_id"]))
        sample=self.db.execute(
            "SELECT source,position_id,context,brain,closed_ms,payload "
            "FROM coordinator_broker_samples").fetchone()
        if with_admission:
            self.db.execute(
                "INSERT INTO coordinator_broker_admissions VALUES(?,?,?,?,?,?,?,?)",
                (sample["source"],sample["position_id"],sample["context"],
                 sample["brain"],sample["closed_ms"],
                 self.admitted if admitted is None else admitted,
                 hashlib.sha256(sample["payload"].encode()).hexdigest(),
                 "g6g-runtime-same-transaction-v1"))
        self.fx.decision(member=[self.fx.member(1)])
        return sample

    def check(self):
        return g6g.audit(self.db,self.fx.manifest,now_ms=fixture.NOW)

    def test_hash_and_first_local_admission_support_past_expert_label(self):
        self.add()
        r=self.check()
        self.assertEqual(r["admission_rows_in_scope"],1)
        self.assertEqual(r["valid_hashed_expert_admissions_in_scope"],1)
        self.assertTrue(r["local_admission_consistency"])
        self.assertEqual(r["broker_sample_claims_exceeding_prior_local_admissions"],0)
        self.assertFalse(r["external_trusted_timestamp"])
        self.assertFalse(r["independent_oos"])
        self.assertFalse(r["model_promotion_allowed"])

    def test_legacy_label_with_no_first_admission_is_unknown_not_qualified(self):
        self.add(with_admission=False)
        r=self.check()
        self.assertFalse(r["local_admission_consistency"])
        self.assertEqual(r["unattested_or_legacy_claimed_sample_mass"],1)
        self.assertEqual(r["broker_sample_claims_exceeding_prior_local_admissions"],1)

    def test_post_decision_admission_cannot_be_used_for_past_training(self):
        self.add(admitted=self.observed+1000)
        r=self.check()
        self.assertFalse(r["local_admission_consistency"])
        self.assertEqual(r["valid_hashed_expert_admissions_in_scope"],1)
        self.assertEqual(r["unattested_or_legacy_claimed_sample_mass"],1)

    def test_tampered_payload_hash_blocks_credit_even_if_net_looks_profitable(self):
        self.add()
        self.db.execute(
            "UPDATE coordinator_broker_samples "
            "SET payload=json_set(payload,'$.gross',100000)")
        r=self.check()
        self.assertEqual(r["valid_hashed_expert_admissions_in_scope"],0)
        self.assertEqual(r["rejected_admissions_by_reason"]
                         ["broker_learning_label_changed_after_admission"],1)
        self.assertFalse(r["local_admission_consistency"])

    def test_invalid_clock_before_original_broker_close_rejected(self):
        self.add(admitted=self.closed-1)
        r=self.check()
        self.assertIn("invalid_local_admission_clock_or_protocol",
                      r["rejected_admissions_by_reason"])

    def test_wrong_mode_or_broker_member_never_counts_as_expert_admission(self):
        self.add()
        self.db.execute(
            "UPDATE coordinator_broker_samples "
            "SET payload=json_set(payload,'$.trading_mode','sniper')")
        sample=self.db.execute("SELECT source,position_id,payload "
                               "FROM coordinator_broker_samples").fetchone()
        self.db.execute(
            "UPDATE coordinator_broker_admissions SET payload_sha256=?",
            (hashlib.sha256(sample["payload"].encode()).hexdigest(),))
        r=self.check()
        self.assertEqual(r["rejected_admissions_by_reason"]
                         ["admission_source_or_mode_mismatch"],1)
        self.assertFalse(r["local_admission_consistency"])

    def test_absent_admission_table_fails_closed_without_claiming_zero_trades(self):
        self.add(with_admission=False)
        self.db.execute("DROP TABLE coordinator_broker_admissions")
        r=self.check()
        self.assertIn("admission_table_not_present",r["rejected_admissions_by_reason"])
        self.assertFalse(r["local_admission_consistency"])
        self.assertFalse(r["paired_executed_broker_comparator"])


if __name__=="__main__":
    unittest.main()
