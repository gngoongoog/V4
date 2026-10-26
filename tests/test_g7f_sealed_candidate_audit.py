"""G7-F source integrity independent of a hypothetical MT5 execution."""
import json
import sqlite3
import sys
import unittest
from pathlib import Path

sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"scripts"))
sys.path.insert(0,str(Path(__file__).resolve().parent))
from g7f_sealed_candidate_audit import audit,canon,sha,PROTOCOL
from test_g7_demo_broker_lineage import make_db,CTX,RUN,BAR,OBS,as_json


def setup(*,with_receipt=True):
    db,adaptive=make_db()
    db.executescript("""
        CREATE TABLE g7f_sealed_candidates(
            run_id TEXT PRIMARY KEY,context TEXT,mode TEXT,sealed_ms INTEGER,
            checkpoint TEXT,checkpoint_sha256 TEXT,protocol TEXT);
        CREATE TABLE g7f_sealed_decisions(
            run_id TEXT,bar_time INTEGER,observed_ms INTEGER,payload TEXT,
            payload_sha256 TEXT,PRIMARY KEY(run_id,bar_time));
    """)
    snapshot=as_json({"mode":"expert","created_ms":123001,
                      "weights_frozen":True,"broker_permission":False})
    db.execute("INSERT INTO g7f_sealed_candidates VALUES (?,?,?,?,?,?,?)",
               (RUN,CTX,"expert",123001,snapshot,sha(snapshot),PROTOCOL))
    if with_receipt:
        receipt={
            "protocol":PROTOCOL,"run_id":RUN,"context":CTX,"mode":"expert",
            "sealed_ms":123001,"checkpoint_sha256":sha(snapshot),
            "bar_time":BAR,"observed_ms":OBS,
            "sealed_candidate":{"brain":"system","side":-1,"key":"sealed:180","bar_time":BAR},
            "online_adaptive":adaptive,
            "candidate_training_effect":False,"candidate_broker_orders":False,
            "candidate_booked_pnl":None,"online_adaptive_policy_sealed":False
        }
        receipt["sealed_candidate_signal_sha256"]=sha(canon(receipt["sealed_candidate"]))
        receipt["online_adaptive_signal_sha256"]=sha(as_json(adaptive))
        raw=as_json(receipt)
        db.execute("INSERT INTO g7f_sealed_decisions VALUES (?,?,?,?,?)",
                   (RUN,BAR,OBS,raw,sha(raw)))
    return db


class SealedCandidateAuditTests(unittest.TestCase):
    def test_legacy_without_seal_is_not_started_not_sealed(self):
        db,_=make_db()
        with db:
            r=audit(db,now_ms=300000)
            self.assertEqual(r["status"],"not_started")
            self.assertFalse(r["online_adaptive_policy_sealed"])

    def test_valid_candidate_is_shadow_sealed_but_never_broker_executed(self):
        db=setup()
        with db:
            r=audit(db,now_ms=300000)
            self.assertEqual(r["status"],"locally_consistent_candidate_only")
            self.assertEqual(r["counters"]["matching_candidate_receipts"],1)
            self.assertFalse(r["online_adaptive_policy_sealed"])
            self.assertFalse(r["model_promotion_allowed"])
            self.assertIsNone(r["candidate_broker_result"])

    def test_missing_candidate_bar_is_a_provenance_failure(self):
        db=setup(with_receipt=False)
        with db:
            with self.assertRaisesRegex(ValueError,"missing_future_candidate_receipt"):
                audit(db,now_ms=300000)

    def test_changed_snapshot_is_not_repaired_by_a_valid_receipt(self):
        db=setup()
        with db:
            db.execute("UPDATE g7f_sealed_candidates SET checkpoint='{}'")
            with self.assertRaisesRegex(ValueError,"seal_missing_modified"):
                audit(db,now_ms=300000)

    def test_rewritten_booked_candidate_claim_fails_even_rehashed(self):
        db=setup()
        with db:
            raw=db.execute("SELECT payload FROM g7f_sealed_decisions").fetchone()[0]
            payload=json.loads(raw)
            payload["candidate_booked_pnl"]=5000
            changed=as_json(payload)
            db.execute("UPDATE g7f_sealed_decisions SET payload=?,payload_sha256=?",
                       (changed,sha(changed)))
            with self.assertRaisesRegex(ValueError,"forged_or_inconsistent"):
                audit(db,now_ms=300000)

    def test_missing_source_pair_fails_before_candidate_claim(self):
        db=setup()
        with db:
            db.execute("DELETE FROM frozen_coordinator_decisions WHERE run_id=?",(RUN,))
            with self.assertRaises(ValueError):
                audit(db,now_ms=300000)

    def test_candidate_receipt_before_seal_is_not_forward_evidence(self):
        db=setup()
        with db:
            db.execute("UPDATE g7f_sealed_candidates SET sealed_ms=200000,"
                       "checkpoint=?,checkpoint_sha256=?",(as_json({"mode":"expert",
                       "created_ms":200000,"weights_frozen":True,
                       "broker_permission":False}),sha(as_json({"mode":"expert",
                       "created_ms":200000,"weights_frozen":True,
                       "broker_permission":False}))))
            with self.assertRaisesRegex(ValueError,"backfilled_preseal_candidate"):
                audit(db,now_ms=300000)


if __name__=="__main__":
    unittest.main()
