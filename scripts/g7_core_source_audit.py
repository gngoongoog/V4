#!/usr/bin/env python3
"""Read-only G7 Core frozen/adaptive decision evidence audit.

Verifies the *local* SQLite registration, paired source and SHA-256 receipts.
This is decision evidence only; it cannot establish independent timestamps,
broker fills, profit, a randomized allocation, or REAL release readiness.
"""
from __future__ import annotations
import argparse
import collections
import hashlib
import json
import sqlite3
from pathlib import Path

PROTOCOL = "g7-local-paired-decisions-v1"


def sha(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def check(db: sqlite3.Connection) -> dict:
    db.row_factory = sqlite3.Row
    tables = {r[0] for r in db.execute(
        "SELECT name FROM sqlite_master WHERE type='table'")}
    required = {"g7_pair_registrations", "g7_paired_decision_evidence",
                "frozen_coordinator_runs", "frozen_coordinator_decisions"}
    if not required.issubset(tables):
        return {"status": "not_started", "registered_runs": 0, "paired_decisions": 0,
                "reason": "G7 registration/receipt tables not yet installed",
                "release_authorized": False, "booked_broker_trades": 0}
    registrations = db.execute("""
        SELECT g.*,f.context AS frozen_context,f.mode AS frozen_mode,
               f.created_ms AS frozen_created_ms,f.snapshot AS snapshot
        FROM g7_pair_registrations g JOIN frozen_coordinator_runs f ON f.id=g.run_id
        ORDER BY g.registered_ms,g.run_id
    """).fetchall()
    orphan = db.execute("""
        SELECT COUNT(*) FROM g7_pair_registrations g
        LEFT JOIN frozen_coordinator_runs f ON g.run_id=f.id WHERE f.id IS NULL
    """).fetchone()[0]
    if orphan:
        raise ValueError("G7 registration without frozen source")
    count = collections.Counter()
    run_results = []
    for run in registrations:
        if (run["protocol"] != PROTOCOL or run["context"] != run["frozen_context"]
                or run["mode"] != run["frozen_mode"]
                or run["registered_ms"] != run["frozen_created_ms"]
                or run["cohort_start_ms"] <= run["registered_ms"]
                or sha(run["snapshot"]) != run["frozen_snapshot_sha256"]):
            raise ValueError("invalid/stale G7 registration: " + run["run_id"])
        pairs = db.execute("""
            SELECT d.bar_time,d.observed_ms,d.frozen,d.adaptive,
                   e.observed_ms AS receipt_observed_ms,e.payload,e.payload_sha256
            FROM frozen_coordinator_decisions d
            LEFT JOIN g7_paired_decision_evidence e
                ON e.run_id=d.run_id AND e.bar_time=d.bar_time
            WHERE d.run_id=? ORDER BY d.bar_time
        """, (run["run_id"],)).fetchall()
        orphan_receipts = db.execute("""
            SELECT COUNT(*) FROM g7_paired_decision_evidence e
            LEFT JOIN frozen_coordinator_decisions d ON d.run_id=e.run_id
                AND d.bar_time=e.bar_time WHERE e.run_id=? AND d.run_id IS NULL
        """, (run["run_id"],)).fetchone()[0]
        if orphan_receipts:
            raise ValueError("G7 receipt without original pair")
        for pair in pairs:
            raw = pair["payload"]
            if not isinstance(raw, str) or sha(raw) != pair["payload_sha256"]:
                raise ValueError("missing/edited G7 decision receipt")
            try:
                value = json.loads(raw)
                frozen = json.loads(pair["frozen"])
                adaptive = json.loads(pair["adaptive"])
            except (TypeError, ValueError) as exc:
                raise ValueError("invalid JSON in G7 source") from exc
            if (value.get("protocol") != PROTOCOL
                    or value.get("run_id") != run["run_id"]
                    or value.get("context") != run["context"]
                    or value.get("mode") != run["mode"]
                    or value.get("frozen_checkpoint_sha256") != run["frozen_snapshot_sha256"]
                    or value.get("observed_ms") != pair["observed_ms"]
                    or value.get("observed_ms") != pair["receipt_observed_ms"]
                    or value.get("bar_time") != pair["bar_time"]
                    or value.get("cohort_start_ms") != run["cohort_start_ms"]
                    or value.get("registered_ms") != run["registered_ms"]
                    or pair["bar_time"] * 1000 <= run["registered_ms"]
                    or pair["observed_ms"] < run["cohort_start_ms"]
                    or value.get("frozen") != frozen or value.get("adaptive") != adaptive
                    or value.get("frozen_signal_sha256") != sha(pair["frozen"])
                    or value.get("adaptive_signal_sha256") != sha(pair["adaptive"])
                    or value.get("execution") != "decision_only_neither_arm_broker_executed"
                    or value.get("booked_pnl", "__missing__") is not None
                    or value.get("source") != "Core/frozen_coordinator.observe"):
                raise ValueError("G7 source/provenance mismatch")
            if value.get("coverage") not in ("local_quote_observed", "quote_missing_or_stale"):
                raise ValueError("unknown G7 quote coverage")
            count[value["coverage"]] += 1
            count["mode_" + run["mode"]] += 1
            if frozen.get("ready") and adaptive.get("ready"):
                count["both_signals_ready"] += 1
            elif frozen.get("ready"):
                count["frozen_only_ready"] += 1
            elif adaptive.get("ready"):
                count["adaptive_only_ready"] += 1
            else:
                count["neither_ready"] += 1
        run_results.append({"run_id": run["run_id"], "mode": run["mode"],
                            "registered_ms": run["registered_ms"], "paired_decisions": len(pairs),
                            "snapshot_sha256": run["frozen_snapshot_sha256"]})
    total_receipts = db.execute("SELECT COUNT(*) FROM g7_paired_decision_evidence").fetchone()[0]
    total_pairs = sum(row["paired_decisions"] for row in run_results)
    if total_receipts != total_pairs:
        raise ValueError("orphan/unregistered G7 evidence")
    return {"status": "inconclusive" if total_pairs else "not_started",
            "registered_runs": len(run_results), "paired_decisions": total_pairs,
            "source_validated_local_only": True, "counts": dict(sorted(count.items())),
            "runs": run_results,
            "booked_broker_trades": 0, "release_authorized": False,
            "warning": "Paired signal records are NOT executed Frozen-vs-Adaptive broker arms, signed time, or independent OOS performance."}


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("sqlite_db", type=Path, help="actual Core SQLite database path")
    p.add_argument("--output", type=Path, help="optional JSON report destination")
    args = p.parse_args()
    if not args.sqlite_db.is_file():
        p.exit(2, "G7 source database does not exist\n")
    # Read-only connection, including WAL contents; never auto-create.
    import urllib.parse
    uri = "file:" + urllib.parse.quote(str(args.sqlite_db.resolve())) + "?mode=ro"
    try:
        with sqlite3.connect(uri, uri=True) as db:
            db.execute("PRAGMA query_only=ON")
            report = check(db)
    except (ValueError, sqlite3.DatabaseError) as exc:
        p.exit(2, f"G7 source verification FAILED: {exc}\n")
    output = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(output, encoding="utf-8")
    else:
        print(output, end="")


if __name__ == "__main__":
    main()
