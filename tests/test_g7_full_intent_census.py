"""G7-C population accounting: all opening intents, not only closed winners."""
import json
import sqlite3
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
sys.path.insert(0, str(Path(__file__).resolve().parent))
from g7_full_intent_census import audit
from test_g7_demo_broker_lineage import (
    make_db, add_command, add_closed, as_json, CTX, CMD, OBS, OPEN, POS
)


def add_extra(db, uid, *, mode="expert", brain="system",
              target="demo", at=243000, valid=True):
    request = {"id":uid,"context":CTX,"brain":brain,"kind":"open",
               "created_ms":at,"server":"Demo","login":17,
               "symbol":"XAUUSD","currency":"USD",
               "target":target,"signal":{"ready":True,"side":-1,"key":"other"},
               "learning":{"trading_mode":mode, "timeframe":"M1", "report_id":""}}
    db.execute("INSERT INTO broker_commands VALUES (?,?,?,?,?,?,?,?)",
               (uid,CTX,brain,"open",at,"queued",as_json(request) if valid else "{broken", "{}"))


class G7CensusTests(unittest.TestCase):
    def test_not_started_without_registered_cohorts(self):
        with sqlite3.connect(":memory:") as db:
            r=audit(db,now_ms=300000)
            self.assertEqual(r["status"],"not_started")
            self.assertFalse(r["release_authorized"])

    def test_one_executed_closed_trade_remains_inconclusive(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            add_closed(db)
            r=audit(db,now_ms=260000)
            self.assertEqual(r["all_open_intents_in_registered_accounts"],1)
            self.assertEqual(r["eligible_system_demo_open_intents"],1)
            self.assertEqual(r["exactly_linked_open_intents"],1)
            self.assertEqual(r["locally_verified_full_demo_closes"],1)
            self.assertAlmostEqual(r["g7b_broker_booked_net_for_verified_closes_only"],9.93)
            self.assertEqual(r["local_command_census_complete"],True)
            self.assertIsNone(r["frozen_broker_pnl"])
            self.assertEqual(r["status"],"inconclusive")

    def test_extra_same_mode_command_stays_denominator_and_unlinked(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            add_closed(db)
            add_extra(db,"unlinked")
            r=audit(db,now_ms=260000)
            self.assertEqual(r["all_open_intents_in_registered_accounts"],2)
            self.assertEqual(r["eligible_system_demo_open_intents"],2)
            self.assertEqual(r["exactly_linked_open_intents"],1)
            self.assertEqual(r["anomalies"]["unmatched_eligible_demo_open_intent"],1)
            self.assertFalse(r["local_command_census_complete"])
            self.assertEqual(r["full_close_fraction_of_eligible_open_intents"],.5)

    def test_open_or_unclosed_trade_does_not_count_as_booked(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            r=audit(db,now_ms=260000)
            self.assertEqual(r["eligible_system_demo_open_intents"],1)
            self.assertEqual(r["matched_unverified_or_unclosed"],1)
            self.assertEqual(r["locally_verified_full_demo_closes"],0)
            self.assertIsNone(r["g7b_broker_booked_net_for_verified_closes_only"])

    def test_nonstudy_mode_and_other_brain_are_counted_not_assigned(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            add_extra(db,"sniper",mode="sniper",at=244000)
            add_extra(db,"other-brain",brain="generated",at=244001)
            r=audit(db,now_ms=260000)
            self.assertEqual(r["all_open_intents_in_registered_accounts"],3)
            self.assertEqual(r["eligible_system_demo_open_intents"],1)
            self.assertEqual(r["counts"]["other_or_unregistered_trading_mode"],1)
            self.assertEqual(r["counts"]["other_brain_open_intent"],1)

    def test_unexpected_live_target_is_explicit_anomaly(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            add_extra(db,"live",target="live")
            r=audit(db,now_ms=260000)
            self.assertEqual(r["all_open_intents_in_registered_accounts"],2)
            self.assertEqual(r["anomalies"]["non_demo_target_in_demo_study"],1)
            self.assertFalse(r["local_command_census_complete"])

    def test_corrupt_unmatched_command_is_not_ignored(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            add_extra(db,"broken",valid=False)
            r=audit(db,now_ms=260000)
            self.assertEqual(r["anomalies"]["invalid_open_intent_identity"],1)
            self.assertEqual(r["all_open_intents_in_registered_accounts"],2)

    def test_no_pairs_does_not_hide_command_issued_after_freeze(self):
        db,_=make_db()
        with db:
            db.execute("DELETE FROM g7_paired_decision_evidence")
            db.execute("DELETE FROM frozen_coordinator_decisions")
            add_extra(db,"unpaired")
            r=audit(db,now_ms=260000)
            self.assertEqual(r["paired_decisions"],0)
            self.assertEqual(r["eligible_system_demo_open_intents"],1)
            self.assertEqual(r["anomalies"]["unmatched_eligible_demo_open_intent"],1)
            self.assertEqual(r["status"],"inconclusive")

    def test_conflicting_command_mode_and_learning_mode_is_anomaly(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            add_extra(db,"mode-disagreement")
            raw=db.execute("SELECT payload FROM broker_commands WHERE id='mode-disagreement'").fetchone()[0]
            cmd=json.loads(raw)
            cmd["trading_mode"]="swing"
            db.execute("UPDATE broker_commands SET payload=? WHERE id='mode-disagreement'",
                       (as_json(cmd),))
            r=audit(db,now_ms=260000)
            self.assertEqual(r["anomalies"]["invalid_open_intent_identity"],1)
            self.assertFalse(r["local_command_census_complete"])

    def test_orphan_postcohort_broker_label_cannot_be_ignored(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            add_closed(db)
            original=db.execute("SELECT source,closed_ms,payload FROM coordinator_broker_samples").fetchone()
            label=json.loads(original["payload"])
            label["position_id"]=901
            label["open_command_id"]="unrelated"
            db.execute(
                "INSERT INTO coordinator_broker_samples VALUES (?,?,?,?,?,?)",
                (original["source"],901,CTX,"system",original["closed_ms"],as_json(label)))
            r=audit(db,now_ms=260000)
            self.assertEqual(r["locally_verified_full_demo_closes"],1)
            self.assertEqual(r["anomalies"]["post_cohort_broker_label_without_g7_link"],1)
            self.assertFalse(r["local_command_census_complete"])

    def test_cancelled_intent_not_converted_to_broker_loss_or_win(self):
        db,a=make_db()
        with db:
            add_command(db,a,state="cancelled")
            r=audit(db,now_ms=260000)
            self.assertEqual(r["eligible_system_demo_open_intents"],1)
            self.assertEqual(r["exactly_linked_open_intents"],1)
            self.assertEqual(r["counts"]["matched_not_executed"],1)
            self.assertEqual(r["locally_verified_full_demo_closes"],0)
            self.assertIsNone(r["g7b_broker_booked_net_for_verified_closes_only"])

    def test_corrupted_pair_fails_before_census(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            db.execute("UPDATE frozen_coordinator_decisions SET adaptive='{}'")
            with self.assertRaisesRegex(ValueError,"source/provenance"):
                audit(db,now_ms=260000)

    def test_future_timestamp_is_rejected_not_silently_excluded(self):
        db,a=make_db()
        with db:
            db.execute("UPDATE g7_pair_registrations SET cohort_start_ms=400000")
            with self.assertRaises(ValueError):
                audit(db,now_ms=260000)


if __name__ == "__main__":
    unittest.main()
