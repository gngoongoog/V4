"""Adversarial G6-E cutoff checks; offline synthetic rows, no broker."""
from __future__ import annotations

import importlib.util
import json
import math
from pathlib import Path
import sys
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/"scripts"))
import audit_v25_g6a_preregistered_quotes as g6a
import audit_v25_g6e_frozen_policy_cutoff as g6e
spec=importlib.util.spec_from_file_location(
    "g6e_g5d_fixtures", ROOT/"tests"/"test_v25_g5d_frozen_pairs.py")
fx=importlib.util.module_from_spec(spec)
spec.loader.exec_module(fx)
BAR=fx.BAR
REGISTERED=(BAR-300)*1000
FUTURE=BAR+1800
NOW=(BAR+200*86400)*1000


class TestFrozenPolicyCutoff(unittest.TestCase):
    def setUp(self):
        self.fx=fx.G5DTests()
        self.fx.setUp()
        self.db=self.fx.db
        self.context=self.db.execute(
            "SELECT context FROM frozen_coordinator_runs WHERE id='run-demo'"
        ).fetchone()[0]
        self.cutoff=(BAR-600)*1000
        self.members=[
            {"brain":{"id":"system","timeframe":"M1","report_id":"",
                      "enabled":True},
             "genome":None,"weight":None,"advisor_enabled":False},
            {"brain":{"id":"trend","timeframe":"M1","report_id":"",
                      "enabled":False},
             "genome":None,"weight":{"broker_samples":1,
                                      "forward_samples":0},
             "advisor_enabled":True},
        ]
        self.db.executescript("""
            CREATE TABLE coordinator_broker_samples(
                source TEXT,position_id INTEGER,context TEXT,brain TEXT,
                closed_ms INTEGER,payload TEXT,
                PRIMARY KEY(source,position_id));
        """)
        self.snapshot={"policy":g6a.g5d.POLICY,"mode":"expert",
                       "weights_frozen":True,"broker_permission":False,
                       "created_ms":self.cutoff,
                       "config":{"symbol":"XAUUSD","trading":{"mode":"expert"}},
                       "members":self.members}

    def tearDown(self):
        self.fx.tearDown()

    def manifest(self):
        self.db.execute(
            "UPDATE frozen_coordinator_runs SET snapshot=? WHERE id='run-demo'",
            (json.dumps(self.snapshot),))
        return g6a.register(self.db,"run-demo",now_ms=REGISTERED)

    def receipt(self,closed,*,position=87,source=None,brain="trend",reward=None):
        if source is None:
            source=g6e._source(self.context,brain=brain,timeframe="M1",
                               report_id="")
        p={"context":self.context,"brain":brain,"trading_mode":"expert",
           "evidence_source":"mt5_demo_broker","position_id":position,
           "closed_ms":closed,"gross":2.7,"commission":-0.7,
           "swap":0.0,"fee":0.0,"net":2.0,"net_per_lot":20.0,
           "closed_volume":0.1,"risk_money":5.0,"r_multiple":0.4,
           "reward":math.tanh(0.4) if reward is None else reward}
        self.db.execute(
            "INSERT INTO coordinator_broker_samples VALUES(?,?,?,?,?,?)",
            (source,position,self.context,brain,closed,json.dumps(p)))

    def test_frozen_model_checkpoint_has_upper_bounded_real_demo_training(self):
        self.receipt(self.cutoff-1000)
        m=self.manifest()
        self.fx.add(FUTURE)
        r=g6e.audit(self.db,m,now_ms=NOW)
        self.assertTrue(r["local_frozen_snapshot_integrity_consistent"])
        self.assertTrue(r["broker_training_claims_bounded_by_prior_closes"])
        self.assertEqual(r["members_claiming_pre_freeze_broker_learning"],1)
        self.assertEqual(r["matching_verified_broker_receipts_closed_before_freeze"],1)
        self.assertEqual(r["post_holdout_frozen_ready_signals"],1)
        self.assertEqual(r["post_holdout_valid_matched_quote_paper_pairs"],1)
        self.assertEqual(r["status"],"fixed_policy_quote_paper_cutoff_consistent")
        self.assertFalse(r["adaptive_policy_training_cutoff_enforced"])
        self.assertFalse(r["independent_oos"])
        self.assertIsNone(r["frozen_broker_pnl"])

    def test_future_broker_close_cannot_support_frozen_weight(self):
        self.receipt(self.cutoff+1000)
        m=self.manifest()
        self.fx.add(FUTURE)
        r=g6e.audit(self.db,m,now_ms=NOW)
        self.assertFalse(r["local_frozen_snapshot_integrity_consistent"])
        self.assertEqual(r["snapshot_violations"]
                         ["frozen_broker_sample_claim_exceeds_prior_verified_closes"],1)
        self.assertFalse(r["broker_training_claims_bounded_by_prior_closes"])
        self.assertFalse(r["study_completeness_qualified"])

    def test_forged_demo_reward_cannot_justify_member_training(self):
        self.receipt(self.cutoff-1000,reward=0.99)
        m=self.manifest()
        r=g6e.audit(self.db,m,now_ms=NOW)
        self.assertEqual(r["invalid_or_legacy_broker_receipts"],1)
        self.assertEqual(r["matching_verified_broker_receipts_closed_before_freeze"],0)
        self.assertEqual(r["snapshot_violations"]
                         ["frozen_broker_sample_claim_exceeds_prior_verified_closes"],1)

    def test_scoped_source_with_wrong_broker_mode_cannot_back_expert_weight(self):
        self.receipt(self.cutoff-1000)
        self.db.execute(
            "UPDATE coordinator_broker_samples "
            "SET payload=json_set(payload,'$.trading_mode','sniper')",())
        m=self.manifest()
        r=g6e.audit(self.db,m,now_ms=NOW)
        self.assertEqual(r["matching_verified_broker_receipts_closed_before_freeze"],0)
        self.assertEqual(r["invalid_or_legacy_broker_receipts"],1)
        self.assertEqual(r["snapshot_violations"]
                         ["frozen_broker_sample_claim_exceeds_prior_verified_closes"],1)

    def test_wrong_account_or_mode_config_never_becomes_fixed_checkpoint(self):
        self.snapshot["config"]["symbol"]="EURUSD"
        m=self.manifest()
        with self.assertRaisesRegex(ValueError,"frozen_config_account_or_mode_mismatch"):
            g6e.audit(self.db,m,now_ms=NOW)
        self.snapshot["config"]["symbol"]="XAUUSD"
        self.snapshot["config"]["trading"]["mode"]="sniper"
        m=self.manifest()
        with self.assertRaisesRegex(ValueError,"frozen_config_account_or_mode_mismatch"):
            g6e.audit(self.db,m,now_ms=NOW)

    def test_snapshot_mutation_after_preregistration_rejected(self):
        m=self.manifest()
        self.snapshot["members"][1]["weight"]["broker_samples"]=500
        self.db.execute("UPDATE frozen_coordinator_runs SET snapshot=?",
                        (json.dumps(self.snapshot),))
        with self.assertRaisesRegex(ValueError,"frozen_policy_or_account_scope_changed"):
            g6e.audit(self.db,m,now_ms=NOW)

    def test_empty_or_duplicate_frozen_brain_cannot_get_certified(self):
        self.snapshot["members"][1]["brain"]["id"]="system"
        m=self.manifest()
        r=g6e.audit(self.db,m,now_ms=NOW)
        self.assertFalse(r["local_frozen_snapshot_integrity_consistent"])
        self.assertIn("invalid_or_duplicate_frozen_member_identity",
                      r["snapshot_violations"])

    def test_absent_weight_is_not_fraudulently_counted_as_broker_learning(self):
        self.snapshot["members"][1]["weight"]=None
        m=self.manifest()
        self.fx.add(FUTURE)
        r=g6e.audit(self.db,m,now_ms=NOW)
        self.assertTrue(r["local_frozen_snapshot_integrity_consistent"])
        self.assertFalse(r["broker_training_claims_exist"])
        self.assertFalse(r["broker_training_claims_bounded_by_prior_closes"])
        self.assertFalse(r["independent_oos"])

    def test_fixed_early_signals_without_matched_quote_fill_reduce_coverage(self):
        self.receipt(self.cutoff-1000)
        m=self.manifest()
        self.fx.add(FUTURE)
        self.fx.add(FUTURE+60,state="open")
        r=g6e.audit(self.db,m,now_ms=NOW)
        self.assertEqual(r["post_holdout_frozen_ready_signals"],2)
        self.assertEqual(r["post_holdout_valid_matched_quote_paper_pairs"],1)
        self.assertEqual(r["unpaired_or_nonqualified_frozen_ready_opportunities"],1)
        self.assertFalse(r["independent_oos"])


if __name__=="__main__":
    unittest.main()
