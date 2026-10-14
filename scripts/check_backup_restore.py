"""End-to-end backup/restore contract against a real Rust Core.
Linux CI fixture only: no MT5 connection and no broker execution.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "debug" / "braintrade-core"
BASE = "http://127.0.0.1:18741"


def main():
    if os.name == "nt":
        raise SystemExit("This restore integration fixture uses the Linux development IPC key.")
    with tempfile.TemporaryDirectory(prefix="braintrade-restore-") as directory:
        root = Path(directory)
        env = dict(os.environ, BRAINTRADE_DATA_DIR=directory, BRAINTRADE_NEWS_OFFLINE="1")
        env.pop("BRAINTRADE_GATEWAY_SCRIPT", None)
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        process = None
        key = ""

        def api(path, body=None):
            headers = {"Content-Type": "application/json", "Authorization": "Bearer " + key}
            request = urllib.request.Request(
                BASE + path,
                data=json.dumps(body).encode() if body is not None else None,
                headers=headers,
            )
            with opener.open(request, timeout=3) as response:
                return json.load(response)

        def expect_400(path, body):
            try:
                api(path, body)
            except urllib.error.HTTPError as error:
                assert error.code == 400, error.code
                return json.load(error)
            raise AssertionError(f"Expected HTTP 400 from {path}")

        def start():
            nonlocal process, key
            process = subprocess.Popen([str(BINARY)], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            for _ in range(80):
                try:
                    key = (root / "ipc.key").read_text()
                    return api("/v1/status")
                except (OSError, urllib.error.URLError):
                    time.sleep(0.1)
            out, err = process.communicate(timeout=2)
            raise AssertionError(f"Core startup failed: {out!r} {err!r}")

        try:
            status = start()
            assert not status["execution_enabled"]
            config = api("/v1/config")["config"]
            risk = dict(config["risk"])
            risk["risk_percent"] = 0.31
            api("/v1/risk", risk)

            created = api("/v1/backup", {})
            name = created["name"]
            assert created["schema"] == 10
            assert (root / "backups" / name / "manifest.json").is_file()
            manifest = json.loads((root / "backups" / name / "manifest.json").read_text())
            assert manifest["restore_implemented"] is True
            assert manifest["secrets_included"] is False
            assert manifest["hash_algorithm"] == "fnv1a64-v1"

            listing = api("/v1/backups")
            match = next(item for item in listing["backups"] if item["name"] == name)
            assert match["valid"] is True and match["restore_supported"] is True
            checked = api("/v1/backup/restore/validate", {"name": name})
            assert checked["valid"] is True and checked["secrets_restored"] is False

            expect_400("/v1/backup/restore/validate", {"name": "../events.sqlite"})
            corrupt = root / "backups" / "corrupt-fixture"
            shutil.copytree(root / "backups" / name, corrupt)
            (corrupt / "config.json").write_text("{}")
            expect_400("/v1/backup/restore/validate", {"name": "corrupt-fixture"})
            assert api("/v1/backup/restore/status")["pending"] is False

            risk["risk_percent"] = 0.42
            api("/v1/risk", risk)
            assert api("/v1/config")["config"]["risk"]["risk_percent"] == 0.42

            staged = api("/v1/backup/restore/stage", {"name": name})
            assert staged["staged"] is True
            assert staged["restart_required"] is True
            assert staged["core_will_shutdown"] is True
            assert staged["secrets_restored"] is False
            rollback = staged["rollback"]
            process.wait(timeout=5)
            assert process.returncode == 0

            status = start()
            assert not status["execution_enabled"]
            restored = api("/v1/config")["config"]
            assert restored["risk"]["risk_percent"] == 0.31
            assert api("/v1/backup/restore/status")["pending"] is False
            last = json.loads((root / "restore.last.json").read_text())
            assert last["status"] == "applied" and last["source"] == name
            assert last["rollback"] == rollback and last["rollback_used"] is False
            listing = api("/v1/backups")
            rb = next(item for item in listing["backups"] if item["name"] == rollback)
            assert rb["valid"] is True and rb["restore_supported"] is True
            assert not list((root / "backups" / name).glob("*.sealed"))
            assert not (root / "backups" / name / "ipc.key").exists()
            events = api("/v1/events")["events"]
            assert any(event["kind"] == "backup_restore_result" and event["detail"]["status"] == "applied" for event in events)

            api("/v1/shutdown", {})
            process.wait(timeout=5)
            assert process.returncode == 0
            print("PASS: backup schema/hash validation, traversal/corruption rejection, staged restart restore, pre-restore rollback snapshot, secret exclusion, restored config and audit result")
        finally:
            if process is not None and process.poll() is None:
                process.terminate()
                process.wait(timeout=5)


if __name__ == "__main__":
    main()
