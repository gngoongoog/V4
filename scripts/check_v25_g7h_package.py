#!/usr/bin/env python3
"""G7-H V25 portable completeness: offline, no broker, no Python imports."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

G7_AUDIT_FILES = (
    "g7_core_source_audit.py", "g7_demo_broker_lineage.py",
    "g7_full_intent_census.py", "g7_cohort_overlap_gate.py",
    "g7_holdout_report.py", "g7f_sealed_candidate_audit.py",
    "g7g_restart_continuity.py", "g7_demo_acceptance_packet.py",
    "audit_v25_g5d_frozen_pairs.py",
    "audit_v25_g6a_preregistered_quotes.py",
    "audit_v25_g6b_executed_demo_arm.py",
)
FILES = (
    "BrainTradeNext.exe",
    "runtime/python/python.exe",
    "Run-G7-Acceptance.cmd",
    "Tools/Run-G7-Acceptance.ps1",
    "G7_H_ACCEPTANCE_AR.md",
    "USER_MANUAL_V25_AR.md",
    "USER_MANUAL_V25_AR.txt",
    *(f"Tools/G7/{name}" for name in G7_AUDIT_FILES),
)
LAUNCHER_REQUIRED = (
    "runtime\\python\\python.exe", "G7\\g7_demo_acceptance_packet.py",
    "--db", "--output", "PYTHONDONTWRITEBYTECODE",
)
LAUNCHER_FORBIDDEN = (
    "order_send(", "/v1/start", "/v1/stop", "MetaTrader5.initialize",
    "Invoke-RestMethod", "Invoke-WebRequest",
)


def sha_file(p: Path) -> str:
    h = hashlib.sha256()
    with p.open("rb") as fd:
        for block in iter(lambda: fd.read(1024*1024), b""):
            h.update(block)
    return h.hexdigest()


def verify(portable: Path, source: Path | None = None) -> dict:
    portable = portable.resolve()
    checks: dict[str, str] = {}
    for rel in FILES:
        file = portable / rel
        checks["present:" + rel] = (
            "PASS" if file.is_file() and file.resolve().is_relative_to(portable)
            else "FAIL"
        )
    launcher = portable/"Tools/Run-G7-Acceptance.ps1"
    cmd = portable/"Run-G7-Acceptance.cmd"
    if launcher.is_file() and cmd.is_file():
        txt = launcher.read_text(encoding="utf-8-sig")
        cmds = cmd.read_text(encoding="utf-8-sig")
        checks["launcher_readonly"] = (
            "PASS" if all(token in txt for token in LAUNCHER_REQUIRED)
            and not any(token in txt for token in LAUNCHER_FORBIDDEN)
            and "Run-G7-Acceptance.ps1" in cmds
            and "/v1/start" not in cmds
            else "FAIL"
        )
    else:
        checks["launcher_readonly"] = "FAIL"
    if source is not None:
        source = source.resolve()
        for name in G7_AUDIT_FILES:
            shipped = portable/"Tools/G7"/name
            original = source/"scripts"/name
            checks["same_bytes:" + name] = (
                "PASS" if shipped.is_file() and original.is_file()
                and sha_file(shipped)==sha_file(original) else "FAIL"
            )
    return {
        "schema":"v25-g7h-portable-validation-v1",
        "checks":checks,
        "status":"PASS" if all(v=="PASS" for v in checks.values()) else "FAIL",
        "real_mt5_demo": "NOT TESTED",
        "model_edge_oos": "NOT PROVEN",
        "real_authorized": False,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--portable", type=Path, required=True)
    parser.add_argument("--source", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = verify(args.portable, args.source)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n",
                               encoding="utf-8")
    print(json.dumps({"status":report["status"],
                      "failed":[k for k,v in report["checks"].items() if v=="FAIL"]}))
    return int(report["status"] != "PASS")


if __name__=="__main__":
    raise SystemExit(main())
