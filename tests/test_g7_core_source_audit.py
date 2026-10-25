"""Adversarial read-only Core G7 evidence checks; no broker involved."""
import json
import sqlite3
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from g7_core_source_audit import PROTOCOL, check, sha


def fixture():
    db = sqlite3.connect(":memory:")
    db.executescript("""
      CREATE TABLE frozen_coordinator_runs(
        id TEXT PRIMARY KEY,context TEXT,mode TEXT,created_ms INTEGER,
        snapshot TEXT,active INTEGER);
      CREATE TABLE frozen_coordinator_decisions(
        run_id TEXT,bar_time INTEGER,observed_ms INTEGER,
        frozen TEXT,adaptive TEXT,PRIMARY KEY(run_id,bar_time));
      CREATE TABLE g7_pair_registrations(
        run_id TEXT PRIMARY KEY,context TEXT,mode TEXT,registered_ms INTEGER,
        cohort_start_ms INTEGER,frozen_snapshot_sha256 TEXT,protocol TEXT);
      CREATE TABLE g7_paired_decision_evidence(
        run_id TEXT,bar_time INTEGER,observed_ms INTEGER,payload TEXT,
        payload_sha256 TEXT,PRIMARY KEY(run_id,bar_time));
    """)
    snapshot = '{"policy":"frozen"}'
    db.execute("INSERT INTO frozen_coordinator_runs VALUES (?,?,?,?,?,?)",
               ("r1", "demo-account-17", "expert", 1000, snapshot, 1))
    db.execute("INSERT INTO g7_pair_registrations VALUES (?,?,?,?,?,?,?)",
               ("r1", "demo-account-17", "expert", 1000, 1001, sha(snapshot), PROTOCOL))
    frozen = '{"ready":true,"side":1}'
    adaptive = '{"ready":true,"side":-1}'
    bar, observed = 2, 61000
    db.execute("INSERT INTO frozen_coordinator_decisions VALUES (?,?,?,?,?)",
               ("r1", bar, observed, frozen, adaptive))
    payload = {"protocol": PROTOCOL, "run_id": "r1", "context": "demo-account-17",
               "mode": "expert", "registered_ms": 1000, "cohort_start_ms": 1001,
               "frozen_checkpoint_sha256": sha(snapshot),
               "bar_time": bar, "observed_ms": observed,
               "frozen_signal_sha256": sha(frozen), "adaptive_signal_sha256": sha(adaptive),
               "frozen": json.loads(frozen), "adaptive": json.loads(adaptive),
               "coverage": "local_quote_observed",
               "source": "Core/frozen_coordinator.observe",
               "execution": "decision_only_neither_arm_broker_executed",
               "booked_pnl": None}
    raw = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    db.execute("INSERT INTO g7_paired_decision_evidence VALUES (?,?,?,?,?)",
               ("r1", bar, observed, raw, sha(raw)))
    db.commit()
    return db


class G7CoreAuditTest(unittest.TestCase):
    def test_valid_local_signal_pair_is_never_broker_evidence(self):
        with fixture() as db:
            result = check(db)
            self.assertEqual(result["status"], "inconclusive")
            self.assertEqual(result["paired_decisions"], 1)
            self.assertEqual(result["booked_broker_trades"], 0)
            self.assertFalse(result["release_authorized"])
            self.assertEqual(result["counts"]["both_signals_ready"], 1)

    def test_missing_new_receipt_detected(self):
        with fixture() as db:
            db.execute("DELETE FROM g7_paired_decision_evidence")
            with self.assertRaisesRegex(ValueError, "missing/edited"):
                check(db)

    def test_changed_source_signal_detected(self):
        with fixture() as db:
            db.execute("UPDATE frozen_coordinator_decisions SET adaptive='{}'")
            with self.assertRaisesRegex(ValueError, "source/provenance"):
                check(db)

    def test_changed_frozen_snapshot_detected(self):
        with fixture() as db:
            db.execute("UPDATE frozen_coordinator_runs SET snapshot='{}'")
            with self.assertRaisesRegex(ValueError, "registration"):
                check(db)

    def test_changed_receipt_detected(self):
        with fixture() as db:
            db.execute("UPDATE g7_paired_decision_evidence SET payload='{}'")
            with self.assertRaisesRegex(ValueError, "missing/edited"):
                check(db)

    def test_receipt_with_local_booked_claim_detected_even_if_rehashed(self):
        with fixture() as db:
            raw = db.execute("SELECT payload FROM g7_paired_decision_evidence").fetchone()[0]
            value = json.loads(raw)
            value["booked_pnl"] = 5000.0
            tampered = json.dumps(value)
            db.execute("UPDATE g7_paired_decision_evidence SET payload=?,payload_sha256=?",
                       (tampered, sha(tampered)))
            with self.assertRaisesRegex(ValueError, "source/provenance"):
                check(db)

    def test_out_of_order_clock_detected(self):
        with fixture() as db:
            db.execute("UPDATE g7_pair_registrations SET cohort_start_ms=70000")
            with self.assertRaisesRegex(ValueError, "source/provenance"):
                check(db)

    def test_legacy_database_without_g7_is_not_started(self):
        with sqlite3.connect(":memory:") as db:
            result = check(db)
            self.assertEqual(result["status"], "not_started")
            self.assertFalse(result["release_authorized"])


if __name__ == "__main__":
    unittest.main()
