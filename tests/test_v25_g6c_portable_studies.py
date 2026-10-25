"""G6-C: list Expert runs without leaking Core account identifiers."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
SPEC=importlib.util.spec_from_file_location(
    "g6c_listing",ROOT/"scripts"/"v25_g6c_list_frozen_runs.py")
import sys
sys.path.insert(0,str(ROOT/"scripts"))
mod=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(mod)


class G6CPortableStudies(unittest.TestCase):
    def test_list_missing_db_tables_returns_no_false_runs(self):
        db=sqlite3.connect(":memory:")
        try:
            s=mod.list_studies(db)
            self.assertFalse(s["available"])
            self.assertEqual(s["studies"],[])
            self.assertTrue(s["read_only"])
        finally:
            db.close()

    def test_list_expert_only_and_hash_account_without_disclosing_login(self):
        db=sqlite3.connect(":memory:")
        try:
            db.executescript("CREATE TABLE frozen_coordinator_runs("
                "id TEXT,context TEXT,mode TEXT,created_ms INTEGER,"
                "snapshot TEXT,active INTEGER);")
            context='["Broker-PRIVATE",999888777,"XAUUSD","USD"]'
            snapshot={"policy":mod.POLICY,"created_ms":1_760_000_000_000,
                      "weights_frozen":True,"broker_permission":False}
            db.execute("INSERT INTO frozen_coordinator_runs VALUES(?,?,?,?,?,1)",
                       ("run-expert",context,"expert",snapshot["created_ms"],
                        json.dumps(snapshot)))
            db.execute("INSERT INTO frozen_coordinator_runs VALUES(?,?,?,?,?,1)",
                       ("run-sniper",context,"sniper",snapshot["created_ms"],
                        json.dumps(snapshot)))
            s=mod.list_studies(db)
            self.assertTrue(s["available"])
            self.assertEqual(len(s["studies"]),1)
            x=s["studies"][0]
            self.assertTrue(x["can_register_new_study"])
            self.assertEqual(x["run_id"],"run-expert")
            self.assertEqual(len(x["account_scope_sha256_prefix"]),16)
            self.assertNotIn("999888777",json.dumps(s))
            self.assertNotIn("Broker-PRIVATE",json.dumps(s))
        finally:
            db.close()

    def test_invalid_or_inactive_snapshot_cannot_be_registered(self):
        db=sqlite3.connect(":memory:")
        try:
            db.execute("CREATE TABLE frozen_coordinator_runs("
                "id TEXT,context TEXT,mode TEXT,created_ms INTEGER,"
                "snapshot TEXT,active INTEGER)")
            db.execute("INSERT INTO frozen_coordinator_runs VALUES("
                "'legacy','[]','expert',1760000000000,'{}',1)")
            db.execute("INSERT INTO frozen_coordinator_runs VALUES("
                "'stopped','[]','expert',1760000000000,?,0)",
                (json.dumps({"policy":mod.POLICY,"created_ms":1760000000000,
                             "weights_frozen":True,"broker_permission":False}),))
            rows=mod.list_studies(db)["studies"]
            self.assertEqual(len(rows),2)
            self.assertTrue(all(not r["can_register_new_study"] for r in rows))
        finally:
            db.close()

    def test_package_verifier_rejects_missing_study_scripts_without_python(self):
        spec=importlib.util.spec_from_file_location(
            "g6c_checker",ROOT/"scripts"/"check_v25_g6c_study_package.py")
        checker=importlib.util.module_from_spec(spec)
        spec.loader.exec_module(checker)
        with tempfile.TemporaryDirectory() as directory:
            result=checker.verify(Path(directory),exec_help=False)
            self.assertEqual(result["status"],"FAIL")
            self.assertIn("missing=bundled-python",result["failures"])
            self.assertFalse(result["live_readiness"])
            self.assertFalse(result["mt5_demo_proven"])


if __name__=="__main__":
    unittest.main()
