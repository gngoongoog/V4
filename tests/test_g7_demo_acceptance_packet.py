"""G7-H end-to-end acceptance packet: never silently convert missing evidence to pass."""
import json
import sqlite3
import sys
import unittest
import subprocess
import tempfile
from contextlib import closing
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
sys.path.insert(0, str(Path(__file__).resolve().parent))
from g7_demo_acceptance_packet import summarize
from test_g7f_sealed_candidate_audit import setup
from test_g7_demo_broker_lineage import add_command, add_closed, RUN
from test_g7g_restart_continuity import saved_pair


def seeded(*,closed=False):
    db=setup()
    adaptive=json.loads(db.execute(
        "SELECT adaptive FROM frozen_coordinator_decisions WHERE run_id=?",
        (RUN,)).fetchone()[0])
    add_command(db,adaptive)
    if closed:
        add_closed(db)
    return db


class G7HAcceptanceTests(unittest.TestCase):
    def test_no_core_tables_cannot_fake_an_active_study(self):
        with sqlite3.connect(":memory:") as db:
            packet=summarize(db,now_ms=350000)
            self.assertEqual(packet["status"],"not_started")
            self.assertIn("g7_research_not_registered",packet["blockers"])
            self.assertFalse(packet["release_authorized"])

    def test_source_only_still_requires_execution_proof(self):
        db=setup()
        with db:
            packet=summarize(db,now_ms=350000)
            self.assertEqual(packet["status"],"insufficient_local_demo_evidence")
            self.assertIn("no_eligible_system_demo_broker_open_intents",
                          packet["blockers"])
            self.assertFalse(packet["native_windows_mt5_demo_session_verified"])

    def test_open_but_unclosed_broker_command_is_not_booked(self):
        db=seeded(closed=False)
        with db:
            packet=summarize(db,now_ms=350000)
            self.assertEqual(packet["counts"]["eligible_demo_open_intents"],1)
            self.assertEqual(packet["counts"]["locally_verified_closed_adaptive_positions"],0)
            self.assertIn("no_exactly_reconciled_fully_closed_demo_positions",
                          packet["blockers"])
            self.assertFalse(packet["training_improved_broker_returns_proven"])

    def test_closed_demo_still_never_authorizes_live(self):
        db=seeded(closed=True)
        with db:
            packet=summarize(db,now_ms=350000)
            self.assertEqual(packet["status"],"awaiting_native_mt5_demo_acceptance")
            self.assertEqual(packet["counts"]["locally_verified_closed_adaptive_positions"],1)
            self.assertEqual(packet["coverage"]["demo_open_intents_with_full_close_fraction"],1)
            self.assertIn("prespecified_holdout_manifest_not_supplied",packet["blockers"])
            self.assertFalse(packet["release_authorized"])
            self.assertFalse(packet["paired_executed_frozen_control"])
            self.assertIsNone(packet["coverage"]["closed_subset_win_rate_descriptive"])
            self.assertNotIn("context",str(packet))
            self.assertNotIn("Demo",str(packet).replace("MT5 Demo",""))

    def test_changed_core_decision_fails_closed_first(self):
        db=seeded(closed=True)
        with db:
            db.execute("UPDATE frozen_coordinator_decisions SET adaptive='{}'")
            with self.assertRaisesRegex(ValueError,"source/provenance"):
                summarize(db,now_ms=350000)

    def test_changed_candidate_receipt_fails_closed(self):
        db=seeded(closed=True)
        with db:
            db.execute("UPDATE g7f_sealed_decisions SET payload='{}'")
            with self.assertRaisesRegex(ValueError,"changed_or_late_candidate_receipt"):
                summarize(db,now_ms=350000)

    def test_valid_pre_restart_snapshot_separately_validates(self):
        after=seeded(closed=True)
        after.commit()
        before=sqlite3.connect(":memory:")
        before.row_factory=sqlite3.Row
        after.backup(before)
        with after,before:
            result=summarize(after,now_ms=350000,previous_db=before)
            self.assertEqual(result["local_validation"]["g7g_restart"],
                             "locally_consistent_restart_only")
            self.assertNotIn("pre_restart_sqlite_snapshot_not_supplied",
                             result["blockers"])
            self.assertIn("native_mt5_demo_forward_trial_not_independently_confirmed",
                          result["blockers"])

    def test_new_future_bars_and_restart_do_not_change_original(self):
        after=seeded(closed=True)
        after.commit()
        before=sqlite3.connect(":memory:")
        before.row_factory=sqlite3.Row
        after.backup(before)
        with after,before:
            saved_pair(after,240)
            result=summarize(after,now_ms=450000,previous_db=before)
            self.assertEqual(result["paired_source_decisions"],2)
            self.assertEqual(result["counts"]["sealed_candidate_forward_receipts"],2)

    def test_real_cli_creates_redacted_not_started_report_and_never_mutates_source(self):
        with tempfile.TemporaryDirectory() as temp:
            home=Path(temp)
            dbfile=home/"events.sqlite"
            output=home/"acceptance.json"
            with closing(sqlite3.connect(dbfile)) as db, db:
                db.execute("CREATE TABLE untouched_marker(v TEXT NOT NULL)")
                db.execute("INSERT INTO untouched_marker VALUES ('preserved')")
            code=subprocess.run(
                [sys.executable,str(Path(__file__).resolve().parents[1]/
                                     "scripts/g7_demo_acceptance_packet.py"),
                 "--db",str(dbfile),"--output",str(output)],
                capture_output=True,text=True,timeout=15)
            self.assertEqual(code.returncode,0,code.stderr)
            data=json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(data["status"],"not_started")
            self.assertFalse(data["release_authorized"])
            with closing(sqlite3.connect(dbfile)) as db, db:
                self.assertEqual(db.execute("SELECT v FROM untouched_marker").fetchone()[0],
                                 "preserved")
            second=subprocess.run(
                [sys.executable,str(Path(__file__).resolve().parents[1]/
                                     "scripts/g7_demo_acceptance_packet.py"),
                 "--db",str(dbfile),"--output",str(output)],
                capture_output=True,text=True,timeout=15)
            self.assertNotEqual(second.returncode,0)
            self.assertEqual(data,json.loads(output.read_text(encoding="utf-8")))

    def test_cli_rejects_same_database_as_report_path(self):
        with tempfile.TemporaryDirectory() as temp:
            dbfile=Path(temp)/"events.sqlite"
            with closing(sqlite3.connect(dbfile)) as db, db:
                db.execute("CREATE TABLE preserve(v INTEGER)")
                db.execute("INSERT INTO preserve VALUES (7)")
            attempt=subprocess.run(
                [sys.executable,str(Path(__file__).resolve().parents[1]/
                                     "scripts/g7_demo_acceptance_packet.py"),
                 "--db",str(dbfile),"--output",str(dbfile)],
                capture_output=True,text=True,timeout=15)
            self.assertNotEqual(attempt.returncode,0)
            with closing(sqlite3.connect(dbfile)) as db, db:
                self.assertEqual(db.execute("SELECT v FROM preserve").fetchone()[0],7)

    def test_historical_loss_or_pair_forgery_is_rejected(self):
        after=seeded(closed=True)
        after.commit()
        before=sqlite3.connect(":memory:")
        before.row_factory=sqlite3.Row
        after.backup(before)
        with after,before:
            after.execute("DELETE FROM g7f_sealed_decisions")
            with self.assertRaises(ValueError):
                summarize(after,now_ms=350000,previous_db=before)


if __name__ == "__main__":
    unittest.main()
