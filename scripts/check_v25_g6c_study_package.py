#!/usr/bin/env python3
"""Verify the real V25 Portable ships executable, source-identical G6 tools.

Build acceptance only: cannot connect MT5, validate broker outcomes, or
exercise native PowerShell dialogs. No writes to app/core databases.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

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
    ("scripts/v25_g6c_list_frozen_runs.py",
     "Tools/G6/v25_g6c_list_frozen_runs.py"),
    ("scripts/Run-G6-Study.ps1", "Tools/Run-G6-Study.ps1"),
    ("scripts/Run-G6-Study.cmd", "Run-G6-Study.cmd"),
    ("docs/v25/G6C_PORTABLE_STUDY_OPERATOR_AR.md", "G6_STUDY_AR.md"),
)


def sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


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
    return {
        "component":"v25-g6c-portable-offline-studies",
        "status":"PASS" if not failures else "FAIL",
        "bundle_is_source_identical":not failures,
        "bundled_python_help_executed":exec_help and not failures,
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
