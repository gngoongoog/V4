"""G6-D evidence may show causal counts, never assert independent OOS."""
from __future__ import annotations

import importlib.util
import json
import math
from pathlib import Path
import sqlite3
import sys
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/"scripts"))
import audit_v25_g6a_preregistered_quotes as g6a
import audit_v25_g6d_causal_learning_evidence as g6d
spec=importlib.util.spec_from_file_location(
    "g6d_g5d_fixture",ROOT/"tests"/"test_v25_g5d_frozen_pairs.py")
fx=importlib.util.module_from_spec(spec)
spec.loader.exec_module(fx)
BAR=fx.BAR
REGISTERED=(BAR-300)*1000
FUTURE=BAR+1800
NOW=(BAR+200*86_400)*1000


class TestCausalMemberLearning(unittest.TestCase):
    def setUp(self):
        self.fx=fx.G5DTests()
        self.fx.setUp()
        self.db=self.fx.db
        self.manifest=g6a.register(self.db,"run-demo",now_ms=REGISTERED)
        self.context=self.db.execute(
            "SELECT context FROM frozen_coordinator_runs WHERE id='run-demo'").fetchone()[0]
        self.db.executescript("""
            CREATE TABLE coordinator_broker_samples (
              source TEXT,position_id INTEGER,context TEXT,brain TEXT,
              closed_ms INTEGER,payload TEXT,PRIMARY KEY(source,position_id));
        """)

    def tearDown(self):
        self.fx.tearDown()

    def member(self,count,weight=1.1,brain="trend"):
        return {"id":brain,"timeframe":"M1","report_id":"",
                "weight":weight,"broker_samples":count}

    def decision(self,bar=FUTURE,member=None):
        items=[self.member(0)] if member is None else member
        self.fx.add(bar,lambda d,p:d["adaptive"].update({
            "coordination":{"members":items}
        }))

    def receipt(self,closed_ms,*,position=80,source=None,context=None):
        member=self.member(1)
        if source is None:
            source=g6d._source(self.context,member)
        if context is None:
            context=self.context
        data={"evidence_source":"mt5_demo_broker",
              "context":context,"brain":"trend",
              "position_id":position,"closed_ms":closed_ms,
              "gross":1.0,"commission":-0.10,"swap":0.0,"fee":0.0,
              "net":0.9,"net_per_lot":9.0,"closed_volume":0.1,
              "risk_money":3.0,"r_multiple":0.3,"reward":math.tanh(0.3)}
        self.db.execute(
            "INSERT INTO coordinator_broker_samples VALUES(?,?,?,?,?,?)",
            (source,position,self.context,"trend",closed_ms,json.dumps(data)))

    def test_claimed_one_broker_label_is_verified_from_past_not_just_ui(self):
        observed=fx.sample(FUTURE)[0]
        self.receipt(observed-1000)
        self.decision(member=[self.member(1)])
        result=g6d.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(result["member_claims_checked"],1)
        self.assertEqual(result["invalid_claims_by_reason"],{})
        self.assertEqual(result["members_reporting_nonzero_broker_samples"],1)
        self.assertEqual(result["status"],"causal_member_counter_consistency_checked")
        self.assertTrue(result["training_evidence_consistency"])
        self.assertFalse(result["claim_that_weight_change_was_caused_by_learning"])
        self.assertFalse(result["independent_oos"])
        self.assertIsNone(result["broker_pnl"])

    def test_future_broker_close_cannot_justify_earlier_displayed_learning(self):
        observed=fx.sample(FUTURE)[0]
        self.receipt(observed+1000)
        self.decision(member=[self.member(1)])
        result=g6d.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(result["member_claims_checked"],0)
        self.assertEqual(
            result["invalid_claims_by_reason"]
                ["claimed_broker_training_exceeds_prior_verified_receipts"],1)
        self.assertFalse(result["training_evidence_consistency"])

    def test_forged_reward_or_wrong_account_cannot_back_fake_training(self):
        observed=fx.sample(FUTURE)[0]
        self.receipt(observed-1000)
        self.db.execute("UPDATE coordinator_broker_samples "
                        "SET payload=json_set(payload,'$.reward',0.99)",())
        self.decision(member=[self.member(1)])
        result=g6d.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(result["excluded_broker_labels_by_reason"]
                         ["incorrect_broker_cost_or_reward_lineage"],1)
        self.assertFalse(result["training_evidence_consistency"])

    def test_non_broker_weights_and_context_changes_not_misreported_as_training(self):
        self.decision(bar=FUTURE,member=[self.member(0,1.0)])
        self.decision(bar=FUTURE+60,member=[self.member(0,1.3)])
        result=g6d.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(result["member_sample_count_increase_observations"],0)
        self.assertEqual(result["members_with_changed_first_to_last_displayed_weights"],1)
        self.assertEqual(result["members_reporting_nonzero_broker_samples"],0)
        self.assertFalse(result["claim_that_weight_change_was_caused_by_learning"])

    def test_broker_count_progress_after_cutoff_is_online_not_static_oos(self):
        first=fx.sample(FUTURE)[0]
        later=fx.sample(FUTURE+60)[0]
        self.receipt(first+1000)
        self.decision(bar=FUTURE,member=[self.member(0)])
        self.decision(bar=FUTURE+60,member=[self.member(1,1.25)])
        result=g6d.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(result["member_sample_count_increase_observations"],1)
        self.assertEqual(result["count_increases_with_post_cutoff_labels_available"],1)
        self.assertFalse(result["training_cutoff_independently_enforced"])
        self.assertFalse(result["adaptive_policy_sealed_across_study"])
        self.assertFalse(result["independent_oos"])

    def test_corrupted_coordination_is_rejected_without_changing_core_db(self):
        self.decision()
        self.db.execute("UPDATE frozen_coordinator_decisions "
                        "SET adaptive=json_set(adaptive,'$.coordination','garbled')",())
        result=g6d.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(result["invalid_claims_by_reason"]
                         ["missing_adaptive_member_ledger"],1)
        self.assertFalse(result["model_promotion_allowed"])

    def test_wrong_member_source_and_invalid_weight_are_explicitly_rejected(self):
        self.decision(member=[self.member(2,weight=float("nan"))])
        result=g6d.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(result["invalid_claims_by_reason"]
                         ["unverified_member_training_counter_or_weight"],1)
        self.assertFalse(result["training_evidence_consistency"])

    def test_missing_broker_ledger_not_mistaken_for_zero_training(self):
        self.db.execute("DROP TABLE coordinator_broker_samples")
        self.decision()
        result=g6d.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(result["status"],"no_broker_learner_evidence_table")
        self.assertIsNone(result["training_evidence_consistency"])
        self.assertFalse(result["model_promotion_allowed"])


if __name__=="__main__":
    unittest.main()
