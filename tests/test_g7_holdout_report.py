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
    # The old bar 180 opened BEFORE holdout start 200000 and must remain
    # excluded. Add a genuinely future complete bar 240, closed at 300000.
    future_bar=240
    future_observed=301000
    old_frozen=json.loads(paired["frozen"])
    adaptive=dict(adaptive,bar_time=future_bar,key="bar:240")
    old_frozen["bar_time"]=future_bar
    old_frozen["key"]="bar:240"
    f,a=as_json(old_frozen),as_json(adaptive)
    db.execute("INSERT INTO frozen_coordinator_decisions VALUES (?,?,?,?,?)",
               (RUN,future_bar,future_observed,f,a))
    proof=json.loads(receipt["payload"])
    proof.update(bar_time=future_bar,observed_ms=future_observed,
                 frozen_signal_sha256=__import__("hashlib").sha256(f.encode()).hexdigest(),
                 adaptive_signal_sha256=__import__("hashlib").sha256(a.encode()).hexdigest(),
                 frozen=old_frozen,adaptive=adaptive)
    raw=as_json(proof)
    from g7_core_source_audit import sha
    db.execute("INSERT INTO g7_paired_decision_evidence VALUES (?,?,?,?,?)",
               (RUN,future_bar,future_observed,raw,sha(raw)))
    return db,adaptive,doc


def add_future_command(db, adaptive):
    add_command(db,adaptive)
    raw=db.execute("SELECT payload FROM broker_commands WHERE id=?",(CMD,)).fetchone()[0]
    data=json.loads(raw)
    data["created_ms"]=301000
    db.execute("UPDATE broker_commands SET created_ms=?,payload=? WHERE id=?",
               (301000,as_json(data),CMD))


def add_future_close(db):
    add_closed(db)
    for ticket in (701,702):
        raw=db.execute("SELECT payload FROM broker_deals WHERE ticket=?",(ticket,)).fetchone()[0]
        p=json.loads(raw)
        p["time_msc"]+=60000
        db.execute("UPDATE broker_deals SET payload=? WHERE ticket=?",
                   (as_json(p),ticket))
    row=db.execute("SELECT source,payload FROM coordinator_broker_samples").fetchone()
    payload=json.loads(row["payload"])
    payload["closed_ms"]=305000
    newraw=as_json(payload)
    db.execute("UPDATE coordinator_broker_samples SET closed_ms=?,payload=? WHERE source=?",
               (305000,newraw,row["source"]))
    from g7_core_source_audit import sha
    db.execute("UPDATE coordinator_broker_admissions SET closed_ms=?,admitted_ms=?,payload_sha256=? WHERE source=?",
               (305000,310000,sha(newraw),row["source"]))


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
            add_future_command(db,a)
            add_future_close(db)
            report=audit(db,doc,now_ms=600000)
            self.assertEqual(report["matched_verified_fully_closed_demo_positions"],1)
            self.assertEqual(report["eligible_system_demo_open_intents"],1)
            self.assertEqual(report["complete_broker_outcome_coverage"],1)
            self.assertEqual(report["intent_level_win_fraction_extreme_bounds_not_execution_win_rate"],[1.0,1.0])
            self.assertIn("new_broker_training_labels_admitted_after_cutoff",
                          report["descriptive_qualification_blockers"])
            self.assertAlmostEqual(report["verified_closed_mean_net_r"],9.93/5)
            self.assertEqual(report["status"],"inconclusive")
            self.assertFalse(report["independent_out_of_sample"])
            self.assertFalse(report["adaptive_policy_sealed"])
            self.assertIsNone(report["frozen_broker_pnl"])
            self.assertIsNone(report["paired_broker_delta"])

    def test_unmatched_open_intent_remains_in_denominator(self):
        db,a,doc=preregister_fixture()
        with db:
            add_future_command(db,a)
            add_future_close(db)
            add_extra(db,"extra",at=280000)
            r=audit(db,doc,now_ms=600000)
            self.assertEqual(r["eligible_system_demo_open_intents"],2)
            self.assertEqual(r["matched_verified_fully_closed_demo_positions"],1)
            self.assertEqual(r["unverified_or_unmatched_eligible_open_intents"],
                             {"unmatched_holdout_open_intent":1})
            self.assertEqual(r["complete_broker_outcome_coverage"],.5)
            self.assertEqual(r["intent_level_win_fraction_extreme_bounds_not_execution_win_rate"],[.5,1.])
            self.assertIn("not_all_eligible_intents_fully_closed_and_attributed",
                          r["descriptive_qualification_blockers"])

    def test_censored_open_position_has_no_pnl(self):
        db,a,doc=preregister_fixture()
        with db:
            add_future_command(db,a)
            r=audit(db,doc,now_ms=600000)
            self.assertEqual(r["matched_verified_fully_closed_demo_positions"],0)
            self.assertIsNone(r["verified_closed_win_rate"])
            self.assertIsNone(r["verified_closed_win_rate_wilson_95_descriptive"])
            self.assertEqual(r["intent_level_win_fraction_extreme_bounds_not_execution_win_rate"],[0.,1.])
            self.assertIsNone(r["daily_cluster_bootstrap_mean_net_r_95_descriptive"])

    def test_cutoff_admission_is_flagged_not_passed_as_independent(self):
        db,a,doc=preregister_fixture()
        with db:
            add_future_command(db,a)
            add_future_close(db)
            r=audit(db,doc,now_ms=600000)
            self.assertEqual(r["new_broker_training_labels_admitted_after_cutoff"],1)
            self.assertFalse(r["independent_out_of_sample"])
            self.assertFalse(r["sufficient_local_descriptive_sample"])

    def test_manifest_tampered_even_when_numbers_look_better(self):
        db,a,doc=preregister_fixture()
        with db:
            add_future_command(db,a)
            add_future_close(db)
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
            add_future_command(db,a)
            # The audit clock must already include the future bar observed
            # at 301000; an earlier now would be a forged/inconsistent view.
            r=audit(db,doc,now_ms=400000)
            self.assertEqual(r["status"],"holdout_in_progress")
            self.assertFalse(r["release_authorized"])

    def test_holdout_late_close_is_not_credited(self):
        db,a,doc=preregister_fixture()
        with db:
            add_future_command(db,a)
            add_future_close(db)
            doc["holdout_end_ms"]=304000
            doc["manifest_sha256"]=manifest_hash(doc)
            r=audit(db,doc,now_ms=600000)
            self.assertEqual(r["matched_verified_fully_closed_demo_positions"],0)
            self.assertEqual(r["unverified_or_unmatched_eligible_open_intents"],
                             {"booked_or_admitted_after_holdout_end":1})

    def test_modified_adaptive_source_fails_before_statistics(self):
        db,a,doc=preregister_fixture()
        with db:
            add_future_command(db,a)
            add_future_close(db)
            db.execute("UPDATE frozen_coordinator_decisions SET adaptive='{}'")
            with self.assertRaisesRegex(ValueError,"source/provenance"):
                audit(db,doc,now_ms=600000)

    def test_pre_registration_command_is_out_of_cohort(self):
        db,a,doc=preregister_fixture()
        with db:
            add_future_command(db,a)
            db.execute("UPDATE broker_commands SET created_ms=124000 WHERE id=?",(CMD,))
            r=audit(db,doc,now_ms=600000)
            self.assertEqual(r["eligible_system_demo_open_intents"],0)

    def test_wilson_zero_one_small_samples_not_overconfident(self):
        self.assertIsNone(wilson95(0,0))
        lo,hi=wilson95(1,1)
        self.assertLess(lo,.30)
        self.assertAlmostEqual(hi,1.0)
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
