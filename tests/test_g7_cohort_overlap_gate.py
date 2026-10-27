"""Adversarial G7-D study epoch and attribution collision tests."""
import json
import sqlite3
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
sys.path.insert(0, str(Path(__file__).resolve().parent))
from g7_cohort_overlap_gate import gate
from g7_full_intent_census import audit as census
from g7_core_source_audit import PROTOCOL, sha
from test_g7_demo_broker_lineage import make_db, add_command, add_closed, CTX, RUN, as_json


def more_cohort(db, *, uid="run2", mode="expert", registered=280000,
                bar=300, observed=361000, active=1):
    """Genuine-shaped Core frozen registration/receipt for a distinct study."""
    snap=as_json({"policy":"frozen"})
    old=db.execute("SELECT frozen,adaptive FROM frozen_coordinator_decisions "
                   "WHERE run_id=?",(RUN,)).fetchone()
    frozen=json.loads(old["frozen"])
    adaptive=json.loads(old["adaptive"])
    db.execute("INSERT INTO frozen_coordinator_runs VALUES (?,?,?,?,?,?)",
               (uid,CTX,mode,registered,snap,active))
    db.execute("INSERT INTO g7_pair_registrations VALUES (?,?,?,?,?,?,?)",
               (uid,CTX,mode,registered,registered+1,sha(snap),PROTOCOL))
    db.execute("INSERT INTO frozen_coordinator_decisions VALUES (?,?,?,?,?)",
               (uid,bar,observed,as_json(frozen),as_json(adaptive)))
    payload = {
        "protocol": PROTOCOL, "run_id": uid, "context": CTX, "mode": mode,
        "registered_ms": registered, "cohort_start_ms": registered+1,
        "frozen_checkpoint_sha256": sha(snap),
        "bar_time": bar, "observed_ms": observed,
        "frozen_signal_sha256": sha(as_json(frozen)),
        "adaptive_signal_sha256": sha(as_json(adaptive)),
        "frozen": frozen, "adaptive": adaptive,
        "coverage":"local_quote_observed",
        "source":"Core/frozen_coordinator.observe",
        "execution":"decision_only_neither_arm_broker_executed",
        "booked_pnl":None
    }
    raw=as_json(payload)
    db.execute("INSERT INTO g7_paired_decision_evidence VALUES (?,?,?,?,?)",
               (uid,bar,observed,raw,sha(raw)))


class G7DStudyIsolationTests(unittest.TestCase):
    def test_no_cohorts_not_a_certified_success(self):
        with sqlite3.connect(":memory:") as db:
            result=gate(db,now_ms=500000)
            self.assertEqual(result["overlap_gate"],"not_started")
            self.assertFalse(result["release_authorized"])

    def test_one_future_study_is_only_local_consistency(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            add_closed(db)
            result=gate(db,now_ms=500000)
            self.assertEqual(result["overlap_gate"],"locally_consistent")
            self.assertEqual(result["paired_decisions"],1)
            self.assertFalse(result["frozen_executed_broker_control"])
            self.assertFalse(result["release_authorized"])
            self.assertEqual(census(db,now_ms=500000)["overlap_gate"],"locally_consistent")

    def test_two_sequential_stopped_and_new_registered_epochs_are_allowed(self):
        db,_=make_db()
        with db:
            db.execute("UPDATE frozen_coordinator_runs SET active=0 WHERE id=?",(RUN,))
            more_cohort(db)
            result=gate(db,now_ms=500000)
            self.assertEqual(result["paired_decisions"],2)
            self.assertEqual(result["overlap_gate"],"locally_consistent")
            self.assertEqual(census(db,now_ms=500000)["paired_decisions"],2)

    def test_old_run_observation_after_new_registration_is_rejected(self):
        db,_=make_db()
        with db:
            db.execute("UPDATE frozen_coordinator_runs SET active=0 WHERE id=?",(RUN,))
            more_cohort(db,registered=230000,bar=300,observed=361000)
            with self.assertRaisesRegex(ValueError,"old_study_decision_after_next_registration"):
                gate(db,now_ms=500000)
            with self.assertRaisesRegex(ValueError,"old_study_decision_after_next_registration"):
                census(db,now_ms=500000)

    def test_duplicate_mode_registration_clock_is_ambiguous(self):
        db,_=make_db()
        with db:
            db.execute("UPDATE frozen_coordinator_runs SET active=0 WHERE id=?",(RUN,))
            more_cohort(db,registered=123000,bar=300,observed=361000)
            with self.assertRaisesRegex(ValueError,"ambiguous_same_mode_registration_boundary"):
                gate(db,now_ms=500000)

    def test_two_active_same_mode_policies_are_impossible(self):
        db,_=make_db()
        with db:
            more_cohort(db,registered=280000)
            with self.assertRaisesRegex(ValueError,"multiple_active_frozen_studies"):
                gate(db,now_ms=500000)

    def test_two_modes_recording_same_bar_same_core_cycle_ambiguous(self):
        db,_=make_db()
        with db:
            more_cohort(db,uid="swing",mode="swing",registered=150000,
                        bar=180,observed=241000)
            with self.assertRaisesRegex(ValueError,"cross_mode_same_cycle"):
                gate(db,now_ms=500000)

    def test_modified_frozen_evidence_fails_before_aggregation(self):
        db,_=make_db()
        with db:
            db.execute("UPDATE g7_paired_decision_evidence SET payload='{}'")
            with self.assertRaisesRegex(ValueError,"missing/edited"):
                gate(db,now_ms=500000)

    def test_future_observation_rejected(self):
        db,_=make_db()
        with db:
            with self.assertRaisesRegex(ValueError,"nonprospective_or_late_decision"):
                gate(db,now_ms=200000)

    def test_outside_eighty_second_bar_window_rejected(self):
        db,_=make_db()
        with db:
            db.execute("UPDATE frozen_coordinator_decisions SET observed_ms=331100")
            # G7-A protects the old payload/time before G7-D's stricter window.
            with self.assertRaisesRegex(ValueError,"source/provenance"):
                gate(db,now_ms=500000)

    def test_two_system_broker_labels_same_physical_position_rejected(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            add_closed(db)
            base=db.execute("SELECT * FROM coordinator_broker_samples").fetchone()
            label=json.loads(base["payload"])
            label["open_command_id"]="other"
            db.execute("INSERT INTO coordinator_broker_samples VALUES(?,?,?,?,?,?)",
                       (base["source"]+"-duplicate",base["position_id"],
                        CTX,"system",base["closed_ms"],as_json(label)))
            with self.assertRaisesRegex(ValueError,"multiple_system_labels_for_same_physical_position"):
                gate(db,now_ms=500000)
            with self.assertRaisesRegex(ValueError,"multiple_system_labels_for_same_physical_position"):
                census(db,now_ms=500000)


if __name__=="__main__":
    unittest.main()
