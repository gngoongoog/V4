"""P0-B/G2 read-only structural acceptance check for a staged V25 Portable.

Runs offline in CI or on the user's extracted Windows folder. Does not import
MetaTrader5, contact Core, send orders or execute PowerShell/EXE files.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

REQUIRED_TOOLS = (
    "Run-Demo-Acceptance.cmd",
    "Tools/Run-Demo-Acceptance.ps1",
    "Tools/Run-Demo-Acceptance-GUI.ps1",
    "DEMO_ACCEPTANCE_AR.md",
    "runtime/python/python.exe",
    "runtime/gateway/check_runtime.py",
    "runtime/gateway/demo_readiness.py",
    "G2_DEMO_READINESS_AR.md",
    "runtime/gateway/execution.py",
    "runtime/gateway/position_protection.py",
)
SAFETY_CONTRACTS = {
    "Run-Demo-Acceptance.cmd": (
        'Tools\\Run-Demo-Acceptance.ps1', 'Tools\\Run-Demo-Acceptance-GUI.ps1',
        'powershell.exe', 'if "%~1"==""',
    ),
    "Tools/Run-Demo-Acceptance.ps1": (
        "--demo-acceptance", "runtime\\gateway\\check_runtime.py",
        "runtime-import-check.json", "--source",
    ),
    "Tools/Run-Demo-Acceptance-GUI.ps1": (
        "Run-Demo-Acceptance.ps1", "demo-acceptance-latest.json",
        "READ ONLY", "Export-ShareBundle", "-NonInteractive",
    ),
    "runtime/gateway/check_runtime.py": (
        '--demo-acceptance', 'Demo acceptance refuses a Live execution target',
        'Account/context changed during capture',
        'READ ONLY',
    ),
    "DEMO_ACCEPTANCE_AR.md": ("MT5", "Demo", "Run-Demo-Acceptance.cmd"),
}
# Never start execution while inspecting a shipped Portable. Conservative
# textual checks supplement but cannot replace a full Windows/MT5 Demo run.
FORBIDDEN_LAUNCHER_FRAGMENTS = (
    "/v1/start", "/v1/stop", "/v1/execution/result", "/v1/execution/authorize",
    "order_send(", "MetaTrader5.initialize", "Invoke-RestMethod", "Invoke-WebRequest",
)


def verify(portable: Path) -> dict:
    portable = portable.resolve()
    checks: dict[str, dict] = {}
    for relative in REQUIRED_TOOLS:
        target = portable / relative
        checks["asset:" + relative] = {
            "status": "PASS" if target.is_file() and target.resolve().is_relative_to(portable)
                      else "FAIL",
            "reason": "present" if target.is_file() else "missing",
        }
    for relative, tokens in SAFETY_CONTRACTS.items():
        target = portable / relative
        if not target.is_file():
            checks["contract:" + relative] = {"status": "FAIL", "reason": "missing"}
            continue
        try:
            source = target.read_text(encoding="utf-8-sig")
            absent = [token for token in tokens if token not in source]
            if relative.endswith((".cmd", ".ps1")):
                absent.extend("forbidden:" + token for token in FORBIDDEN_LAUNCHER_FRAGMENTS
                              if token in source)
        except (OSError, UnicodeError):
            absent = ["unreadable"]
        checks["contract:" + relative] = {
            "status": "FAIL" if absent else "PASS",
            "reason": ", ".join(absent[:7]) if absent else "read-only wiring present",
        }
    return {
        "schema": "v25-p0b-acceptance-package-v1",
        "portable": str(portable),
        "checks": checks,
        "broker_demo_roundtrip": "NOT TESTED",
        "mt5_order_send": "NOT TESTED",
        "ea_metaeditor_compile": "NOT TESTED",
        "ready_for_real": False,
        "note": "Static package structure is not proof of authentic broker fill, Demo reconciliation or working WebView2.",
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Offline V25 Portable Demo Acceptance Center completeness check")
    parser.add_argument("--portable", type=Path, required=True)
    parser.add_argument("--output", type=Path,
                        help="Optional JSON summary output; no secrets or broker data")
    args = parser.parse_args(argv)
    report = verify(args.portable)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: value["status"] for key, value in report["checks"].items()}, indent=2))
    return 1 if any(value["status"] == "FAIL" for value in report["checks"].values()) else 0


if __name__ == "__main__":
    sys.exit(main())
