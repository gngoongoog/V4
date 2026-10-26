"""G7-G adversarial Core restart continuity and immutable evidence comparison."""
import json
import sqlite3
import sys
import unittest
from pathlib import Path

sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"scripts"))
sys.path.insert(0,str(Path(__file__).resolve().parent))
from g7g_restart_continuity import verify
from g7f_sealed_candidate_audit import sha,canon,PROTOCOL
from test_g7f_sealed_candidate_audit import setup
from test_g7_demo_broker_lineage import RUN,as_json


def saved_pair(db,bar):
    assert bar>123
    d=db.execute(
        "SELECT frozen,adaptive FROM frozen_coordinator_decisions "
        "WHERE run_id=? AND bar_time=180",(RUN,)).fetchone()
    frozen=json.loads(d["frozen"])
    adaptive=json.loads(d["adaptive"])
    frozen["bar_time"]=bar
    adaptive["bar_time"]=bar
    rawf,rawa=as_json(frozen),as_json(adaptive)
    observed=(bar+61)*1000
    db.execute("INSERT INTO frozen_coordinator_decisions VALUES (?,?,?,?,?)",
               (RUN,bar,observed,rawf,rawa))
    old_core=json.loads(db.execute(
        "SELECT payload FROM g7_paired_decision_evidence "
        "WHERE run_id=? AND bar_time=180",(RUN,)).fetchone()[0])
    old_core.update(bar_time=bar,observed_ms=observed,frozen=frozen,adaptive=adaptive,
                    frozen_signal_sha256=sha(rawf),adaptive_signal_sha256=sha(rawa))
    core_raw=as_json(old_core)
    db.execute("INSERT INTO g7_paired_decision_evidence VALUES (?,?,?,?,?)",
               (RUN,bar,observed,core_raw,sha(core_raw)))
    old_candidate=json.loads(db.execute(
        "SELECT payload FROM g7f_sealed_decisions "
        "WHERE run_id=? AND bar_time=180",(RUN,)).fetchone()[0])
    candidate=dict(old_candidate["sealed_candidate"])
    candidate["bar_time"]=bar
    old_candidate.update(bar_time=bar,observed_ms=observed,
                         sealed_candidate=candidate,
                         online_adaptive=adaptive,
                         sealed_candidate_signal_sha256=sha(canon(candidate)),
                         online_adaptive_signal_sha256=sha(rawa))
    candidate_raw=as_json(old_candidate)
    db.execute("INSERT INTO g7f_sealed_decisions VALUES (?,?,?,?,?)",
               (RUN,bar,observed,candidate_raw,sha(candidate_raw)))


def before_after():
    left=setup()
    # The fixture's INSERTs begin an implicit write transaction. SQLite's
    # backup API must read from a fully committed source snapshot; otherwise
    # it can keep retrying on the source's own uncommitted writer lock.
    left.commit()
    right=sqlite3.connect(":memory:")
    left.backup(right)
    return left,right


class RestartContinuityTests(unittest.TestCase):
    def test_exact_same_snapshot_is_local_integrity_not_market_skill(self):
        before,after=before_after()
        with before,after:
            outcome=verify(before,after,now_ms=450000)
            self.assertEqual(outcome["status"],"locally_consistent_restart_only")
            self.assertEqual(outcome["preserved_sealed_run_count"],1)
            self.assertFalse(outcome["independent_oos"])
            self.assertFalse(outcome["real_authorized"])
            self.assertEqual(outcome["tables"]["g7f_sealed_decisions"]["added"],0)

    def test_legitimate_later_full_m1_evidence_preserves_old_receipt(self):
        before,after=before_after()
        with before,after:
            saved_pair(after,240)
            report=verify(before,after,now_ms=450000)
            self.assertEqual(report["after_candidate_pairs"],2)
            self.assertEqual(report["tables"]["g7f_sealed_decisions"]["added"],1)
            self.assertTrue(report["no_historical_bar_backfill"])

    def test_a_new_past_bar_is_backfill_not_restart_continuity(self):
        before,after=before_after()
        with before,after:
            saved_pair(after,150)
            with self.assertRaisesRegex(ValueError,"g7g_backfilled_historical_bar"):
                verify(before,after,now_ms=450000)

    def test_deleted_original_brokerless_candidate_cannot_pass(self):
        before,after=before_after()
        with before,after:
            after.execute("DELETE FROM g7f_sealed_decisions WHERE run_id=?",(RUN,))
            with self.assertRaises(ValueError):
                verify(before,after,now_ms=450000)

    def test_mutated_registered_checkpoint_rejected(self):
        before,after=before_after()
        with before,after:
            after.execute("UPDATE g7f_sealed_candidates SET checkpoint='{}' "
                          "WHERE run_id=?",(RUN,))
            with self.assertRaises(ValueError):
                verify(before,after,now_ms=450000)

    def test_edited_old_receipt_hash_even_if_hash_recomputed_cannot_pass(self):
        before,after=before_after()
        with before,after:
            raw=after.execute("SELECT payload FROM g7f_sealed_decisions").fetchone()[0]
            p=json.loads(raw)
            p["sealed_candidate"]["reason"]="retroactively_improved"
            p["sealed_candidate_signal_sha256"]=sha(canon(p["sealed_candidate"]))
            new=as_json(p)
            after.execute("UPDATE g7f_sealed_decisions SET payload=?,payload_sha256=?",
                          (new,sha(new)))
            with self.assertRaisesRegex(ValueError,"mutated_original_row"):
                verify(before,after,now_ms=450000)

    def test_reopened_database_backup_contains_original_seal(self):
        before,after=before_after()
        with before,after:
            self.assertEqual(
                before.execute("SELECT checkpoint FROM g7f_sealed_candidates").fetchone()[0],
                after.execute("SELECT checkpoint FROM g7f_sealed_candidates").fetchone()[0])
            self.assertTrue(verify(before,after,now_ms=450000)["original_rows_immutable"])

    def test_new_pair_without_candidate_receipt_is_not_allowed(self):
        before,after=before_after()
        with before,after:
            saved_pair(after,240)
            after.execute("DELETE FROM g7f_sealed_decisions WHERE bar_time=240")
            with self.assertRaisesRegex(ValueError,"missing_future_candidate_receipt"):
                verify(before,after,now_ms=450000)

    def test_no_candidate_in_first_backup_cannot_claim_continuity(self):
        before,after=before_after()
        with before,after:
            before.execute("DELETE FROM g7f_sealed_decisions")
            before.execute("DELETE FROM g7f_sealed_candidates")
            with self.assertRaisesRegex(ValueError,"no_sealed_candidate"):
                verify(before,after,now_ms=450000)


if __name__=="__main__":
    unittest.main()
