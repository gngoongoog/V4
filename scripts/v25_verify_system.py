"""Read-only V25 P0 acceptance inventory; never initializes MT5 or sends orders.

Run beside an extracted portable folder (or use --portable PATH); optional
--source PATH enables safe, isolated Python gateway regression tests.
"""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
from hashlib import sha256
import html
import json
import os
from pathlib import Path
import subprocess
import sys

REQUIRED = (
    "BrainTradeNext.exe",
    "runtime/python/python.exe",
    "runtime/gateway/check_runtime.py",
    "runtime/gateway/execution.py",
    "mt5/Experts/BrainTradeSniperBridge.mq5",
)


def file_sha256(path: Path) -> str:
    """Stream an immutable shipped file without executing it or requiring RAM for it."""
    digest = sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def verify(portable: Path, source: Path | None = None):
    portable = portable.resolve()
    checks = {}
    files = [p for p in REQUIRED if (portable / p).is_file()]
    checks["portable_required_assets"] = {"status": "PASS" if len(files) == len(REQUIRED) else "FAIL", "present": files, "missing": [p for p in REQUIRED if p not in files]}
    manifest_path = portable / "SHA256SUMS.json"
    if not manifest_path.is_file():
        checks["sha256_manifest"] = {"status": "FAIL", "reason": "SHA256SUMS.json missing"}
    else:
        try:
            manifest = json.loads(manifest_path.read_text(encoding="utf-8-sig"))
            if not isinstance(manifest, dict):
                raise ValueError("SHA256SUMS must be a JSON object")
            failures = []
            missing_count = changed_count = extra_count = unsafe_count = 0
            for name, expected in manifest.items():
                if not isinstance(name, str):
                    unsafe_count += 1
                    failures.append("UNSAFE MANIFEST ENTRY (not a string)")
                    continue
                rel = Path(name)
                if rel.is_absolute() or ".." in rel.parts or "\\" in name or not name:
                    unsafe_count += 1
                    failures.append("UNSAFE MANIFEST ENTRY: " + name[:180])
                    continue
                target = (portable / rel).resolve()
                if not target.is_relative_to(portable):
                    unsafe_count += 1
                    failures.append("UNSAFE PATH: " + name)
                elif not target.is_file():
                    missing_count += 1
                    failures.append("MISSING: " + name)
                elif not isinstance(expected, str) or file_sha256(target).lower() != expected.lower():
                    changed_count += 1
                    failures.append("CHANGED HASH: " + name)
            expected_files = set(manifest)
            actual_files = {str(p.relative_to(portable)).replace(os.sep, "/") for p in portable.rglob("*") if p.is_file() and p.name != manifest_path.name and not "verification-output" in p.parts}
            # The optional read-only V25 verifier may be laid over an immutable
            # V24 portable. It is not part of V24's signed manifest; exclude
            # only these precisely named diagnostics, never arbitrary files.
            diagnostic_overlay = {"VERIFY_SYSTEM.cmd", "Tools/v25_verify_system.py", "V25_P0_DIAGNOSTICS_README_AR.txt"}
            extras = sorted(actual_files - expected_files - diagnostic_overlay)
            extra_count = len(extras)
            failures.extend("EXTRA UNLISTED: " + n for n in extras)
            # IMPORTANT: do not ignore runtime __pycache__, DLL/EXE, or arbitrary
            # sidecars. They could affect executable imports. The original
            # signed files and any unlisted additions must both be checked.
            checks["sha256_manifest"] = {
                "status": "PASS" if not failures else "FAIL",
                "checked_count": len(manifest),
                "problems_total": len(failures),
                "missing_count": missing_count,
                "changed_hash_count": changed_count,
                "extra_unlisted_count": extra_count,
                "unsafe_manifest_count": unsafe_count,
                "problems": failures[:40],
                "diagnosis": (
                    "The 973-style number is the number of manifest entries, "
                    "NOT how many are corrupted. See problems[] for exact paths. "
                    "Keep the original ZIP; extract it into a NEW empty folder "
                    "and run VERIFY_SYSTEM.cmd BEFORE launching the application."
                    if failures else "All packaged manifest files and file inventory match."
                ),
            }
        except (OSError, ValueError, TypeError, json.JSONDecodeError) as error:
            checks["sha256_manifest"] = {"status": "FAIL", "reason": type(error).__name__}
    binary = portable / "BrainTradeNext.exe"
    if binary.is_file():
        checks["exe_fingerprint"] = {"status": "PASS", "sha256": sha256(binary.read_bytes()).hexdigest(), "note": "Identity only; not proof of execution or a new V25 build"}
    else:
        checks["exe_fingerprint"] = {"status": "FAIL", "reason": "Missing executable"}
    runtime_python = portable / "runtime/python/python.exe"
    if sys.platform == "win32" and runtime_python.is_file():
        try:
            proc = subprocess.run([str(runtime_python), str(portable / "runtime/gateway/check_runtime.py")], cwd=portable, text=True, capture_output=True, timeout=30, env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"})
            checks["runtime_import_only"] = {"status": "PASS" if proc.returncode == 0 else "FAIL", "exit_code": proc.returncode}
        except (OSError, subprocess.TimeoutExpired) as error:
            checks["runtime_import_only"] = {"status": "FAIL", "reason": type(error).__name__}
    else:
        checks["runtime_import_only"] = {"status": "NOT TESTED", "reason": "Run on Windows x64 with packaged Python"}
    if source:
        source = source.resolve()
        lock = source / "apps/terminal/package-lock.json"
        checks["source_lockfile"] = {"status": "PASS" if lock.is_file() else "FAIL"}
        # Never use real trading APIs in this local mock suite.
        if (source / "tests/test_execution_journal_race.py").is_file():
            env = {**os.environ, "PYTHONDONTWRITEBYTECODE": "1"}
            try:
                proc = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", "tests", "-p", "test_execution_journal_race.py", "-q"], cwd=source, env=env, text=True, capture_output=True, timeout=60)
                checks["simulated_journal_race"] = {"status": "PASS" if proc.returncode == 0 else "FAIL", "exit_code": proc.returncode}
            except (OSError, subprocess.TimeoutExpired) as error:
                checks["simulated_journal_race"] = {"status": "FAIL", "reason": type(error).__name__}
        else:
            checks["simulated_journal_race"] = {"status": "NOT TESTED", "reason": "Regression test not found"}
    else:
        checks["source_lockfile"] = {"status": "NOT TESTED", "reason": "No source directory supplied"}
        checks["simulated_journal_race"] = {"status": "NOT TESTED", "reason": "No source directory supplied"}
    for key in ("broker_demo_roundtrip", "real_money_trading", "gui_22_pages", "ea_metaeditor_compile", "windows_build_v25"):
        checks[key] = {"status": "NOT TESTED", "reason": "Requires independent Windows, MetaEditor or authenticated Demo evidence"}
    return {"schema": "braintrade-v25-p0-verification-v1", "observed_utc": datetime.now(timezone.utc).isoformat(), "checks": checks, "ready_for_real": False}


def write_report(report: dict, out: Path):
    out.mkdir(parents=True, exist_ok=True)
    (out / "v25-verification.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    def detail(value: dict) -> str:
        reason = value.get("reason")
        problems = value.get("problems", [])
        if problems:
            counts = (
                f"Checked: {value.get('checked_count', 0)} | "
                f"Missing: {value.get('missing_count', 0)} | "
                f"Changed hashes: {value.get('changed_hash_count', 0)} | "
                f"Unlisted extras: {value.get('extra_unlisted_count', 0)} | "
                f"Unsafe: {value.get('unsafe_manifest_count', 0)} | "
                f"TOTAL PROBLEMS: {value.get('problems_total', 0)}"
            )
            items = "".join("<li><code>" + html.escape(str(x)) + "</code></li>" for x in problems[:40])
            return "<strong>" + html.escape(counts) + "</strong><ul>" + items + "</ul>"
        if reason:
            return html.escape(str(reason))
        if value.get("checked_count") is not None:
            return html.escape(f"Verified {value['checked_count']} manifest entries; no problems")
        return "—"
    rows = "\n".join(
        "<tr><td>" + html.escape(name) + "</td><td>" +
        html.escape(v["status"]) + "</td><td>" + detail(v) + "</td></tr>"
        for name, v in report["checks"].items()
    )
    page = "<!doctype html><html lang='ar' dir='rtl'><meta charset='utf-8'><title>BrainTrade V25 P0 verification</title><style>body{background:#171a1e;color:#eee;font:16px sans-serif;margin:40px}table{border-collapse:collapse;width:100%}td,th{border:1px solid #555;padding:12px}th{background:#31363e}</style><h1>BRAINTRADE V25 — P0 Check</h1><p>هذه النتائج ليست اعتماد تداول حقيقي أو دليلاً على الربحية.</p><table><tr><th>الفحص</th><th>النتيجة</th><th>التفاصيل</th></tr>" + rows + "</table></html>"
    (out / "v25-verification.html").write_text(page, encoding="utf-8")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--portable", type=Path, default=Path.cwd())
    ap.add_argument("--source", type=Path)
    ap.add_argument("--output", type=Path, default=Path.cwd() / "verification-output")
    args = ap.parse_args()
    report = verify(args.portable, args.source)
    write_report(report, args.output)
    print(json.dumps({name: value["status"] for name, value in report["checks"].items()}, ensure_ascii=False, indent=2))
    return int(any(item["status"] == "FAIL" for item in report["checks"].values()))


if __name__ == "__main__":
    sys.exit(main())
