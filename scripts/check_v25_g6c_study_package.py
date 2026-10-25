#!/usr/bin/env python3
"""Verify the real V25 Portable ships executable, source-identical G6 tools.

Build acceptance only: cannot connect MT5, validate broker outcomes, or
exercise native PowerShell dialogs. No writes to app/core databases.
"""
from __future__ import annotations

import argparse
from contextlib import closing
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import sqlite3
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
FILES = (
    ("scripts/audit_v25_g5d_frozen_pairs.py",
     "Tools/G6/audit_v25_g5d_frozen_pairs.py"),
    ("scripts/audit_v25_g6a_preregistered_quotes.py",
     "Tools/G6/audit_v25_g6a_preregistered_quotes.py"),
    ("scripts/audit_v25_g6b_executed_demo_arm.py",
     "Tools/G6/audit_v25_g6b_executed_demo_arm.py"),
    ("scripts/audit_v25_g6d_causal_learning_evidence.py",
     "Tools/G6/audit_v25_g6d_causal_learning_evidence.py"),
    ("scripts/audit_v25_g6e_frozen_policy_cutoff.py",
     "Tools/G6/audit_v25_g6e_frozen_policy_cutoff.py"),
    ("scripts/audit_v25_g6g_broker_admission_clock.py",
     "Tools/G6/audit_v25_g6g_broker_admission_clock.py"),
    ("scripts/v25_g6c_list_frozen_runs.py",
     "Tools/G6/v25_g6c_list_frozen_runs.py"),
    ("scripts/Run-G6-Study.ps1", "Tools/Run-G6-Study.ps1"),
    ("scripts/Run-G6-Study.cmd", "Run-G6-Study.cmd"),
    ("docs/v25/G6C_PORTABLE_STUDY_OPERATOR_AR.md", "G6_STUDY_AR.md"),
)


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def _bundled_python_sqlite_study_smoke(runtime_python: Path,
                                       scripts: Path) -> list[str]:
    """Run real bundled-Python List/Register and deny premature paper results.

    Generated account and SQLite are synthetic, kept only in TemporaryDirectory,
    and never contact MT5 or real user data.
    """
    problems=[]
    with tempfile.TemporaryDirectory(prefix="v25-g6-studies-") as directory:
        directory=Path(directory)
        database=directory/"fictional_core.sqlite"
        manifest=directory/"registered.json"
        now=int(time.time()*1000)
        context='["FAKE_BROKER_ACCOUNT_PRIVATE",999888777,"XAUUSD","USD"]'
        snapshot={
            "policy":"frozen-expert-coordinator-decision-v1",
            "mode":"expert","created_ms":now-120_000,
            "weights_frozen":True,"broker_permission":False,
            "config":{"symbol":"XAUUSD","trading":{"mode":"expert"}},
            "members":[{"brain":{"id":"system","timeframe":"M1",
                       "report_id":"","enabled":True},
                        "genome":None,"weight":None,"advisor_enabled":False}],
        }
        # sqlite3.Connection.__exit__ commits but does NOT close the handle.
        # Windows forbids unlinking this temp database until close(), unlike
        # Linux; closing() guarantees no locked test fixture leaks into CI.
        with closing(sqlite3.connect(database)) as db:
            db.executescript("""
                CREATE TABLE frozen_coordinator_runs(
                    id TEXT PRIMARY KEY,context TEXT,mode TEXT,created_ms INTEGER,
                    snapshot TEXT,active INTEGER);
                CREATE TABLE frozen_coordinator_decisions(
                    run_id TEXT,bar_time INTEGER,observed_ms INTEGER,
                    frozen TEXT,adaptive TEXT,PRIMARY KEY(run_id,bar_time));
                CREATE TABLE frozen_coordinator_trade_pairs(
                    run_id TEXT,bar_time INTEGER,opened_ms INTEGER,
                    closed_ms INTEGER,state TEXT,payload TEXT,
                    PRIMARY KEY(run_id,bar_time));
            """)
            db.execute("INSERT INTO frozen_coordinator_runs VALUES(?,?,?,?,?,1)",
                ("synthetic-run",context,"expert",snapshot["created_ms"],
                 json.dumps(snapshot)))
            db.commit()
        def command(script: str, *args: str) -> subprocess.CompletedProcess:
            return subprocess.run(
                [str(runtime_python),str(scripts/script),*map(str,args)],
                check=False,capture_output=True,text=True,timeout=15,
                cwd=str(directory))
        try:
            listed=command("v25_g6c_list_frozen_runs.py","--db",str(database))
            if listed.returncode!=0:
                problems.append(f"list-synthetic-run-failed={listed.stderr[-300:]}")
                return problems
            data=json.loads(listed.stdout)
            if (data.get("studies",[{}])[0].get("run_id")!="synthetic-run"
                    or not data["studies"][0]["can_register_new_study"]
                    or "FAKE_BROKER_ACCOUNT_PRIVATE" in listed.stdout
                    or "999888777" in listed.stdout):
                problems.append("synthetic-list-exposes-account-or-invalid-run")
                return problems
            register=command("audit_v25_g6a_preregistered_quotes.py",
                "register","--db",str(database),"--run-id","synthetic-run",
                "--manifest",str(manifest),"--embargo-minutes","30")
            if register.returncode!=0 or not manifest.is_file():
                problems.append(f"synthetic-preregister-failed={register.stderr[-300:]}")
                return problems
            envelope=json.loads(manifest.read_text(encoding="utf-8"))
            if (not isinstance(envelope,dict)
                    or len(envelope.get("manifest_sha256",""))!=64):
                problems.append("synthetic-registration-hash-invalid")
                return problems
            # Reusing a registration manifest is forbidden; no silent overwrite.
            second=command("audit_v25_g6a_preregistered_quotes.py",
                "register","--db",str(database),"--run-id","synthetic-run",
                "--manifest",str(manifest))
            if second.returncode==0:
                problems.append("synthetic-manifest-overwrite-not-blocked")
                return problems
            premature=command("audit_v25_g6a_preregistered_quotes.py",
                "audit","--db",str(database),"--manifest",str(manifest))
            if (premature.returncode==0
                    or "holdout_has_not_started" not in premature.stderr):
                problems.append("premature-research-was-not-rejected")
        except (OSError,ValueError,KeyError,IndexError,json.JSONDecodeError,
                subprocess.TimeoutExpired) as exc:
            problems.append(f"synthetic-study-exception={type(exc).__name__}")
    return problems


def verify(portable: Path, *, exec_help: bool = True) -> dict:
    failures=[]
    artifacts=[]
    for src, rel in FILES:
        source=ROOT/src
        target=portable/rel
        if not source.is_file() or not target.is_file():
            failures.append(f"missing={rel}")
            continue
        if sha(source)!=sha(target):
            failures.append(f"source-mismatch={rel}")
            continue
        artifacts.append({"path":rel,"sha256":sha(target)})
    runtime_python = portable/"runtime/python/python.exe"
    if not runtime_python.is_file():
        failures.append("missing=bundled-python")
    if not failures and exec_help:
        for script in (
            "v25_g6c_list_frozen_runs.py",
            "audit_v25_g6a_preregistered_quotes.py",
            "audit_v25_g6b_executed_demo_arm.py",
            "audit_v25_g6d_causal_learning_evidence.py",
        ):
            try:
                proc=subprocess.run(
                    [str(runtime_python),str(portable/"Tools/G6"/script),"--help"],
                    check=False,capture_output=True,text=True,timeout=15)
            except (OSError, subprocess.TimeoutExpired) as exc:
                failures.append(f"cannot-start-{script}={type(exc).__name__}")
                continue
            if proc.returncode!=0 or "usage:" not in proc.stdout.lower():
                failures.append(f"help-failed-{script}={proc.returncode}")
    study_smoke_passed=False
    if not failures and exec_help:
        failures.extend(_bundled_python_sqlite_study_smoke(
            runtime_python,portable/"Tools/G6"))
        study_smoke_passed=not failures
    return {
        "component":"v25-g6c-portable-offline-studies",
        "status":"PASS" if not failures else "FAIL",
        "bundle_is_source_identical":not failures,
        "bundled_python_help_executed":exec_help and not failures,
        "bundled_python_synthetic_sqlite_study_passed":study_smoke_passed,
        "files":artifacts,"failures":failures,
        "mt5_demo_proven":False,"broker_trades_opened":0,
        "live_readiness":False,
    }


def main() -> None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--portable",required=True,type=Path)
    parser.add_argument("--output",type=Path)
    args=parser.parse_args()
    result=verify(args.portable)
    text=json.dumps(result,ensure_ascii=False,indent=2,sort_keys=True)
    if args.output:
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(text+"\n",encoding="utf-8")
    print(text)
    if result["status"]!="PASS":
        raise SystemExit(2)


if __name__=="__main__":
    main()
