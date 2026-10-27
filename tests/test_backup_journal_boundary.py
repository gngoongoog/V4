from __future__ import annotations

import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import time
import unittest
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "debug" / "braintrade-core"
BASE = "http://127.0.0.1:18741"


@unittest.skipIf(os.name == "nt", "Linux CI fixture uses the development plaintext IPC key")
@unittest.skipUnless(BINARY.is_file(), "Requires a built local Rust braintrade-core executable")
class BackupJournalBoundaryTests(unittest.TestCase):
    """The durable broker send journal is transport state and must never roll back with Core backups."""

    def test_restore_does_not_copy_or_rewind_broker_send_journal(self):
        with tempfile.TemporaryDirectory(prefix="braintrade-journal-boundary-") as directory:
            root = Path(directory)
            journal = root / "broker-journal.sqlite"
            db = sqlite3.connect(journal)
            db.execute(
                "CREATE TABLE sends(id TEXT PRIMARY KEY, request TEXT NOT NULL, result TEXT, created_ms INTEGER NOT NULL)"
            )
            db.execute(
                "INSERT INTO sends VALUES(?,?,?,?)",
                (
                    "sent-before-backup",
                    json.dumps({"id": "sent-before-backup", "symbol": "XAUUSD"}),
                    json.dumps({"state": "unknown", "reason": "simulated interrupted send"}),
                    123456789,
                ),
            )
            db.commit()
            db.close()
            journal_before = journal.read_bytes()

            env = dict(
                os.environ,
                BRAINTRADE_DATA_DIR=directory,
                BRAINTRADE_GATEWAY_DISABLED="1",
                BRAINTRADE_NEWS_OFFLINE="1",
            )
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            process = None
            key = ""

            def close_process_pipe():
                nonlocal process
                if process is not None and process.stderr is not None:
                    process.stderr.close()

            def api(path, body=None):
                request = urllib.request.Request(
                    BASE + path,
                    data=None if body is None else json.dumps(body).encode(),
                    headers={"Authorization": "Bearer " + key, "Content-Type": "application/json"},
                )
                with opener.open(request, timeout=4) as response:
                    return json.load(response)

            def boot():
                nonlocal process, key
                process = subprocess.Popen(
                    [str(BINARY)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE
                )
                for _ in range(100):
                    try:
                        key = (root / "ipc.key").read_text()
                        api("/v1/status")
                        return
                    except OSError:
                        if process.poll() is not None:
                            break
                        time.sleep(0.1)
                stderr = process.stderr.read().decode(errors="replace") if process.stderr else ""
                close_process_pipe()
                raise AssertionError("Core failed to start: " + stderr)

            try:
                boot()
                config = api("/v1/config")["config"]
                risk = dict(config["risk"])
                risk["risk_percent"] = 0.31
                api("/v1/risk", risk)

                created = api("/v1/backup", {})
                backup_dir = root / "backups" / created["name"]
                self.assertEqual(
                    sorted(path.name for path in backup_dir.iterdir()),
                    ["config.json", "events.sqlite", "manifest.json"],
                )
                manifest = json.loads((backup_dir / "manifest.json").read_text())
                self.assertFalse(manifest["gateway_send_journal_restored"])
                self.assertNotIn("broker-journal.sqlite", manifest["files"])
                self.assertEqual(journal.read_bytes(), journal_before)

                risk["risk_percent"] = 0.42
                api("/v1/risk", risk)
                staged = api("/v1/backup/restore/stage", {"name": created["name"]})
                self.assertTrue(staged["staged"])
                process.wait(timeout=6)
                self.assertEqual(process.returncode, 0)
                close_process_pipe()

                boot()
                restored = api("/v1/config")["config"]
                self.assertEqual(restored["risk"]["risk_percent"], 0.31)
                self.assertEqual(journal.read_bytes(), journal_before)

                check = sqlite3.connect(journal)
                row = check.execute(
                    "SELECT id, request, result, created_ms FROM sends WHERE id=?",
                    ("sent-before-backup",),
                ).fetchone()
                check.close()
                self.assertIsNotNone(row)
                self.assertEqual(row[0], "sent-before-backup")
                self.assertEqual(json.loads(row[2])["state"], "unknown")
                self.assertEqual(row[3], 123456789)

                api("/v1/shutdown", {})
                process.wait(timeout=6)
                self.assertEqual(process.returncode, 0)
                close_process_pipe()
            finally:
                if process is not None and process.poll() is None:
                    process.terminate()
                    process.wait(timeout=5)
                close_process_pipe()


if __name__ == "__main__":
    unittest.main()
