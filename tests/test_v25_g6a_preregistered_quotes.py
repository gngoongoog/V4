"""Adversarial G6-A prospective protocol fixtures. No MT5 or trading account."""
from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path
import sqlite3
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import audit_v25_g6a_preregistered_quotes as study

fixture_spec = importlib.util.spec_from_file_location(
    "g5d_fixture_for_g6a", ROOT / "tests" / "test_v25_g5d_frozen_pairs.py")
fixture = importlib.util.module_from_spec(fixture_spec)
fixture_spec.loader.exec_module(fixture)
BAR = fixture.BAR
REGISTERED = (BAR - 300) * 1000
AFTER_EMBARGO = BAR + 1800
AUDIT_NOW = (BAR + 200 * 86_400) * 1000


class PreRegisteredQuotes(unittest.TestCase):
    def setUp(self):
        self.base = fixture.G5DTests()
        self.base.setUp()
        self.db = self.base.db

    def tearDown(self):
        self.base.tearDown()

    def pre_register(self, embargo=study.MIN_EMBARGO_MS):
        return study.register(self.db, "run-demo",
                              now_ms=REGISTERED, embargo_ms=embargo)

    def test_single_legitimate_pair_has_no_broker_pnl_or_oos_certification(self):
        manifest = self.pre_register()
        self.base.add(AFTER_EMBARGO)
        r = study.audit(self.db, manifest, now_ms=AUDIT_NOW)
        self.assertEqual(r["eligible_post_holdout_pairs"], 1)
        self.assertEqual(r["status"], "insufficient_preregistered_future_quote_pairs")
        self.assertEqual(r["adaptive_minus_frozen_mean_quote_paper_net_per_lot"], 5.0)
        self.assertIsNone(r["broker_pnl"])
        self.assertFalse(r["independent_oos"])
        self.assertFalse(r["adaptive_training_cutoff_independently_locked"])
        self.assertFalse(r["registration_independently_timestamped"])
        self.assertFalse(r["model_promotion_allowed"])

    def test_pre_registration_paper_pair_is_excluded_not_backfilled(self):
        # This sample happened AFTER the original frozen run but BEFORE G6-A.
        self.base.add(BAR - 420)
        m = self.pre_register()
        self.base.add(AFTER_EMBARGO)
        r = study.audit(self.db, m, now_ms=AUDIT_NOW)
        self.assertEqual(r["excluded_pre_holdout_pairs"], 1)
        self.assertEqual(r["eligible_post_holdout_pairs"], 1)
        self.assertEqual(r["adaptive_minus_frozen_mean_quote_paper_net_per_lot"], 5.0)

    def test_future_dated_existing_rows_block_pre_registration(self):
        self.base.add(AFTER_EMBARGO)
        with self.assertRaisesRegex(ValueError, "preexisting_future_dated_rows"):
            self.pre_register()

    def test_missing_or_inactive_frozen_run_cannot_be_preregistered(self):
        with self.assertRaisesRegex(ValueError, "frozen_run_not_found"):
            study.register(self.db, "nonexistent", now_ms=REGISTERED)
        self.db.execute("UPDATE frozen_coordinator_runs SET active=0")
        with self.assertRaisesRegex(ValueError, "run_must_be_frozen_and_active"):
            self.pre_register()

    def test_changing_manifest_or_frozen_weights_after_registration_fails(self):
        m = self.pre_register()
        m2 = copy.deepcopy(m)
        m2["manifest"]["holdout_start_ms"] += 1000
        with self.assertRaisesRegex(ValueError, "manifest_hash_mismatch"):
            study.audit(self.db, m2, now_ms=AUDIT_NOW)
        self.db.execute("UPDATE frozen_coordinator_runs SET snapshot=?",
                        (json.dumps({"policy": study.g5d.POLICY,
                                     "mode": "expert",
                                     "weights_frozen": True,
                                     "broker_permission": False,
                                     "created_ms": (BAR-600)*1000,
                                     "changed_weight":999}),))
        with self.assertRaisesRegex(ValueError, "frozen_policy_or_account_scope_changed"):
            study.audit(self.db, m, now_ms=AUDIT_NOW)

    def test_account_reassignment_is_rejected(self):
        m = self.pre_register()
        self.db.execute("UPDATE frozen_coordinator_runs SET context='other-account'")
        with self.assertRaisesRegex(ValueError, "frozen_policy_or_account_scope_changed"):
            study.audit(self.db, m, now_ms=AUDIT_NOW)

    def test_losing_a_saved_sample_invalidates_watermark(self):
        self.base.add(BAR - 420)
        m = self.pre_register()
        self.db.execute("DELETE FROM frozen_coordinator_trade_pairs")
        with self.assertRaisesRegex(ValueError, "research_rows_removed_after_preregistration"):
            study.audit(self.db, m, now_ms=AUDIT_NOW)

    def test_cost_fraud_after_holdout_is_excluded(self):
        m = self.pre_register()
        self.base.add(AFTER_EMBARGO, lambda d,p: p["frozen"]["result"].update({
            "commission_assumed": 0.0}))
        r = study.audit(self.db, m, now_ms=AUDIT_NOW)
        self.assertEqual(r["eligible_post_holdout_pairs"], 0)
        self.assertEqual(r["rejected_post_holdout_pairs"]["paper_cost_or_result_mismatch"], 1)

    def test_multiple_days_is_only_qualified_quote_paper(self):
        m = self.pre_register()
        for i in range(32):
            self.base.add(AFTER_EMBARGO + i * 6 * 3600)
        r = study.audit(self.db, m, now_ms=AUDIT_NOW)
        self.assertEqual(r["eligible_post_holdout_pairs"], 32)
        self.assertGreaterEqual(r["eligible_distinct_utc_days"], 5)
        self.assertEqual(r["status"], "prospective_quote_paper_descriptive_only")
        self.assertEqual(r["day_block_bootstrap_95pct_quote_paper"], [5.0, 5.0])
        self.assertFalse(r["independent_oos"])
        self.assertIsNone(r["broker_pnl"])
        self.assertEqual(r["mt5_broker_fills_compared"], 0)

    def test_expiration_and_embargo_are_not_optional(self):
        with self.assertRaisesRegex(ValueError, "embargo_out_of_supported_range"):
            self.pre_register(0)
        m = self.pre_register()
        with self.assertRaisesRegex(ValueError, "holdout_has_not_started"):
            study.audit(self.db, m, now_ms=REGISTERED + 1000)

    def test_missing_decision_is_not_qualified(self):
        m = self.pre_register()
        observed, opened, closed, decision, p = fixture.sample(AFTER_EMBARGO)
        self.db.execute("INSERT INTO frozen_coordinator_trade_pairs VALUES(?,?,?,?,?,?)",
                        ("run-demo", AFTER_EMBARGO, opened, closed,
                         "complete", json.dumps(p)))
        r=study.audit(self.db,m,now_ms=AUDIT_NOW)
        self.assertEqual(r["eligible_post_holdout_pairs"],0)
        self.assertEqual(r["excluded_pre_holdout_pairs"],0)
        self.assertEqual(r["rejected_post_holdout_pairs"]["missing_pre_outcome_decision"],1)


if __name__ == "__main__":
    unittest.main()
