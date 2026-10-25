"""G7-E adversarial preregistration, cohort censoring and uncertainty tests."""
import json
import sqlite3
import sys
import unittest
from pathlib import Path

sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"scripts"))
sys.path.insert(0,str(Path(__file__).resolve().parent))
from g7_holdout_report import (
    audit, register, manifest_hash, wilson95, daily_bootstrap_ci
)
from test_g7_demo_broker_lineage import (
    make_db, add_command, add_closed, add_closed as closed,
    CTX, CMD, RUN, OBS, POS, as_json,
)
from test_g7_full_intent_census import add_extra

SHA="c"*40
START=200000
END=500000


def preregister_fixture():
    db,adaptive=make_db()
    # Pretend the first M1 bar has not happened yet: register BEFORE any
    # original paired evidence. Restore the real-shaped Core pair afterward.
    paired=db.execute("SELECT * FROM frozen_coordinator_decisions").fetchone()
    receipt=db.execute("SELECT * FROM g7_paired_decision_evidence").fetchone()
    db.execute("DELETE FROM frozen_coordinator_decisions")
    db.execute("DELETE FROM g7_paired_decision_evidence")
    doc=register(db,run_id=RUN,source_commit=SHA,
                 training_cutoff_ms=123000,
                 holdout_start_ms=START,
                 holdout_end_ms=END,now_ms=123010)
    db.execute("INSERT INTO frozen_coordinator_decisions VALUES (?,?,?,?,?)",
               tuple(paired))
    db.execute("INSERT INTO g7_paired_decision_evidence VALUES (?,?,?,?,?)",
               tuple(receipt))
    return db,adaptive,doc


class HoldoutTests(unittest.TestCase):
    def test_register_refuses_after_first_decision(self):
        db,_=make_db()
        with db:
            with self.assertRaisesRegex(ValueError,"after_first_study_decision"):
                register(db,run_id=RUN,source_commit=SHA,
                         training_cutoff_ms=123000,
                         holdout_start_ms=START,
                         holdout_end_ms=END,now_ms=123010)

    def test_register_refuses_non_sha_or_no_future_embargo(self):
        db,_=make_db()
        with db:
            db.execute("DELETE FROM frozen_coordinator_decisions")
            db.execute("DELETE FROM g7_paired_decision_evidence")
            with self.assertRaisesRegex(ValueError,"source_commit"):
                register(db,run_id=RUN,source_commit="a",
                         training_cutoff_ms=123000,
                         holdout_start_ms=START,
                         holdout_end_ms=END,now_ms=123010)
            with self.assertRaisesRegex(ValueError,"nonprospective"):
                register(db,run_id=RUN,source_commit=SHA,
                         training_cutoff_ms=123000,
                         holdout_start_ms=123100,
                         holdout_end_ms=END,now_ms=123010)

    def test_valid_closed_trade_remains_inconclusive_and_not_oos(self):
        db,a,doc=preregister_fixture()
        with db:
            add_command(db,a)
            add_closed(db)
            report=audit(db,doc,now_ms=600000)
            self.assertEqual(report["matched_verified_fully_closed_demo_positions"],1)
            self.assertEqual(report["eligible_system_demo_open_intents"],1)
            self.assertEqual(report["complete_broker_outcome_coverage"],1)
            self.assertAlmostEqual(report["verified_closed_mean_net_r"],9.93/5)
            self.assertEqual(report["status"],"inconclusive")
            self.assertFalse(report["independent_out_of_sample"])
            self.assertFalse(report["adaptive_policy_sealed"])
            self.assertIsNone(report["frozen_broker_pnl"])
            self.assertIsNone(report["paired_broker_delta"])

    def test_unmatched_open_intent_remains_in_denominator(self):
        db,a,doc=preregister_fixture()
        with db:
            add_command(db,a)
            add_closed(db)
            add_extra(db,"extra",at=280000)
            r=audit(db,doc,now_ms=600000)
            self.assertEqual(r["eligible_system_demo_open_intents"],2)
            self.assertEqual(r["matched_verified_fully_closed_demo_positions"],1)
            self.assertEqual(r["unverified_or_unmatched_eligible_open_intents"],
                             {"unmatched_holdout_open_intent":1})
            self.assertEqual(r["complete_broker_outcome_coverage"],.5)

    def test_censored_open_position_has_no_pnl(self):
        db,a,doc=preregister_fixture()
        with db:
            add_command(db,a)
            r=audit(db,doc,now_ms=600000)
            self.assertEqual(r["matched_verified_fully_closed_demo_positions"],0)
            self.assertIsNone(r["verified_closed_win_rate"])
            self.assertIsNone(r["verified_closed_win_rate_wilson_95_descriptive"])
            self.assertIsNone(r["daily_cluster_bootstrap_mean_net_r_95_descriptive"])

    def test_cutoff_admission_is_flagged_not_passed_as_independent(self):
        db,a,doc=preregister_fixture()
        with db:
            add_command(db,a)
            add_closed(db)
            r=audit(db,doc,now_ms=600000)
            self.assertEqual(r["new_broker_training_labels_admitted_after_cutoff"],1)
            self.assertFalse(r["independent_out_of_sample"])
            self.assertFalse(r["sufficient_local_descriptive_sample"])

    def test_manifest_tampered_even_when_numbers_look_better(self):
        db,a,doc=preregister_fixture()
        with db:
            add_command(db,a)
            add_closed(db)
            doc["min_verified_closes"]=1
            with self.assertRaisesRegex(ValueError,"modified_holdout_manifest_hash"):
                audit(db,doc,now_ms=600000)
            doc["manifest_sha256"]=manifest_hash(doc)
            with self.assertRaisesRegex(ValueError,"invalid_required_holdout_claims"):
                audit(db,doc,now_ms=600000)

    def test_registration_source_context_tampering_rejected(self):
        db,a,doc=preregister_fixture()
        with db:
            db.execute("UPDATE g7_pair_registrations SET context='other' WHERE run_id=?",(RUN,))
            with self.assertRaises(ValueError):
                audit(db,doc,now_ms=600000)

    def test_early_report_is_in_progress_and_not_positive_claim(self):
        db,a,doc=preregister_fixture()
        with db:
            add_command(db,a)
            r=audit(db,doc,now_ms=260000)
            self.assertEqual(r["status"],"holdout_in_progress")
            self.assertFalse(r["release_authorized"])

    def test_holdout_late_close_is_not_credited(self):
        db,a,doc=preregister_fixture()
        with db:
            add_command(db,a)
            add_closed(db)
            doc["holdout_end_ms"]=244000
            doc["manifest_sha256"]=manifest_hash(doc)
            r=audit(db,doc,now_ms=600000)
            self.assertEqual(r["matched_verified_fully_closed_demo_positions"],0)
            self.assertEqual(r["unverified_or_unmatched_eligible_open_intents"],
                             {"booked_or_admitted_after_holdout_end":1})

    def test_modified_adaptive_source_fails_before_statistics(self):
        db,a,doc=preregister_fixture()
        with db:
            add_command(db,a)
            add_closed(db)
            db.execute("UPDATE frozen_coordinator_decisions SET adaptive='{}'")
            with self.assertRaisesRegex(ValueError,"source/provenance"):
                audit(db,doc,now_ms=600000)

    def test_pre_registration_command_is_out_of_cohort(self):
        db,a,doc=preregister_fixture()
        with db:
            add_command(db,a)
            db.execute("UPDATE broker_commands SET created_ms=124000 WHERE id=?",(CMD,))
            r=audit(db,doc,now_ms=600000)
            self.assertEqual(r["eligible_system_demo_open_intents"],0)

    def test_wilson_zero_one_small_samples_not_overconfident(self):
        self.assertIsNone(wilson95(0,0))
        lo,hi=wilson95(1,1)
        self.assertLess(lo,.30)
        self.assertEqual(hi,1.0)
        self.assertAlmostEqual(sum(wilson95(5,10))/2,0.5)

    def test_daily_ci_requires_five_days_and_is_deterministic(self):
        values=[{"closed_ms":i*86_400_000+1000,"net_r":i*.3}
                for i in range(5)]
        self.assertIsNone(daily_bootstrap_ci(values[:4],"f"*64))
        a=daily_bootstrap_ci(values,"f"*64)
        self.assertEqual(a,daily_bootstrap_ci(values,"f"*64))
        self.assertLessEqual(a[0],a[1])


if __name__=="__main__":
    unittest.main()
