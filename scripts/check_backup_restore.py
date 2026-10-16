"""End-to-end backup/restore contract against a real Rust Core.
Linux CI fixture only: no MT5 connection and no broker execution.
"""
import json
import math
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "debug" / "braintrade-core"
BASE = "http://127.0.0.1:18741"
T0 = 1_800_000_000


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

        def saved_history(symbol, returns):
            closes = [100.0]
            for value in returns:
                closes.append(closes[-1] * math.exp(value))
            bars = []
            for index, close in enumerate(closes):
                bars.append({
                    "candle": {
                        "time": T0 + index * 60,
                        "open": close,
                        "high": close,
                        "low": close,
                        "close": close,
                        "tick_volume": 100,
                    },
                    "spread_points": 10,
                })
            last = T0 + len(returns) * 60
            return {
                "symbol": symbol,
                "server": "Demo",
                "login": 7,
                "currency": "USD",
                "captured_ms": (last + 60) * 1000,
                "contract": {
                    "point": 0.01,
                    "tick_size": 0.01,
                    "tick_loss": 1.0,
                    "tick_profit": 1.0,
                    "volume_min": 0.01,
                    "volume_step": 0.01,
                    "volume_max": 100.0,
                },
                "bars": bars,
            }

        def seed_backtest(run_id, symbol, returns):
            report = {
                "symbol": symbol,
                "options": {"from": T0, "to": T0 + (len(returns) + 1) * 60},
            }
            history = saved_history(symbol, returns)
            with sqlite3.connect(root / "events.sqlite", timeout=5) as db:
                db.execute(
                    "INSERT INTO backtest_runs(id,created_ms,report,history) VALUES(?,?,?,?)",
                    (run_id, 1, json.dumps(report, separators=(",", ":")), json.dumps(history, separators=(",", ":"))),
                )
                db.commit()

        try:
            status = start()
            assert not status["execution_enabled"]
            config = api("/v1/config")["config"]
            risk = dict(config["risk"])
            risk["risk_percent"] = 0.31
            api("/v1/risk", risk)
            portfolio_saved = dict(enabled=True, max_positions=2, max_lots=0.25, max_risk_percent=1.5)
            api("/v1/execution/portfolio", portfolio_saved)
            current_portfolio = api("/v1/execution")["portfolio"]
            assert current_portfolio["enabled"] is True
            assert current_portfolio["limits"]["max_positions"] == 2
            assert abs(current_portfolio["limits"]["max_lots"] - 0.25) < 1e-12

            seed_backtest("corr-before-a", "AAA", [0.01, 0.02, -0.01, 0.03])
            seed_backtest("corr-before-b", "BBB", [0.02, 0.04, -0.02, 0.06])
            before = api("/v1/research/correlation/analyze", {
                "source_ids": ["corr-before-a", "corr-before-b"], "min_overlap": 4,
            })
            before_id = before["summary"]["id"]
            assert api("/v1/research/correlation/replay", {"id": before_id})["matches_stored_report"] is True
            before_export = api("/v1/research/correlation/export", {"id": before_id})
            assert before_export["deterministic_replay_verified"] is True
            assert [item["name"] for item in before_export["files"]] == ["report.json", "provenance.json", "pairs.csv"]

            created = api("/v1/backup", {})
            name = created["name"]
            assert created["schema"] == 12
            assert (root / "backups" / name / "manifest.json").is_file()
            manifest = json.loads((root / "backups" / name / "manifest.json").read_text())
            assert manifest["restore_implemented"] is True
            assert manifest["secrets_included"] is False
            assert manifest["broker_execution_included"] is True
            assert manifest["coordinator_samples_included"] is True
            assert manifest["gateway_send_journal_restored"] is False
            assert manifest["hash_algorithm"] == "fnv1a64-v1"

            seed_backtest("corr-after-a", "CCC", [0.01, -0.02, 0.03, -0.01])
            seed_backtest("corr-after-b", "DDD", [-0.01, 0.02, -0.03, 0.01])
            after = api("/v1/research/correlation/analyze", {
                "source_ids": ["corr-after-a", "corr-after-b"], "min_overlap": 4,
            })
            after_id = after["summary"]["id"]
            assert before_id != after_id

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
            sidecar = root / "backups" / name / "events.sqlite-wal"
            sidecar.write_bytes(b"unexpected WAL must remain untouched")
            expect_400("/v1/backup/restore/validate", {"name": name})
            assert sidecar.read_bytes() == b"unexpected WAL must remain untouched"
            sidecar.unlink()
            assert api("/v1/backup/restore/validate", {"name": name})["valid"]

            risk["risk_percent"] = 0.42
            api("/v1/risk", risk)
            assert api("/v1/config")["config"]["risk"]["risk_percent"] == 0.42
            api("/v1/execution/portfolio", dict(enabled=False, max_positions=7, max_lots=7.0, max_risk_percent=7.0))
            assert api("/v1/execution")["portfolio"]["enabled"] is False

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
            restored_portfolio = api("/v1/execution")["portfolio"]
            assert restored_portfolio["enabled"] is True
            assert restored_portfolio["limits"]["max_positions"] == 2
            assert abs(restored_portfolio["limits"]["max_lots"] - 0.25) < 1e-12
            assert abs(restored_portfolio["limits"]["max_risk_percent"] - 1.5) < 1e-12
            assert api("/v1/backup/restore/status")["pending"] is False

            restored_corr = api("/v1/research/correlation/report", {"id": before_id})
            assert restored_corr["id"] == before_id and restored_corr["broker_effect"] is False
            assert api("/v1/research/correlation/replay", {"id": before_id})["matches_stored_report"] is True
            restored_export = api("/v1/research/correlation/export", {"id": before_id})
            assert restored_export == before_export
            expect_400("/v1/research/correlation/report", {"id": after_id})

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
            print("PASS: backup schema/hash validation, traversal/corruption rejection, staged restart restore, correlation replay/export preservation, pre-restore rollback snapshot, secret exclusion, restored config + portfolio exposure limits and audit result")
        finally:
            if process is not None and process.poll() is None:
                process.terminate()
                process.wait(timeout=5)


if __name__ == "__main__":
    main()
