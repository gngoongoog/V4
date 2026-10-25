#!/usr/bin/env python3
"""List G6 candidate Expert frozen Demo runs without printing account identity.

READ ONLY. No MT5, account credentials, file exports or order execution.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sqlite3
import sys

from audit_v25_g5d_frozen_pairs import POLICY


def list_studies(db: sqlite3.Connection) -> dict:
    db.row_factory = sqlite3.Row
    exists = db.execute(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master "
        "WHERE type='table' AND name='frozen_coordinator_runs')"
    ).fetchone()[0]
    if not exists:
        return {"available": False, "studies": [],
                "reason": "No frozen_coordinator_runs table in this database",
                "read_only": True}
    rows = db.execute(
        "SELECT id,context,mode,created_ms,snapshot,active "
        "FROM frozen_coordinator_runs ORDER BY created_ms DESC,id DESC LIMIT 200"
    ).fetchall()
    studies = []
    for r in rows:
        if r["mode"] != "expert" or not isinstance(r["context"], str):
            continue
        try:
            snap = json.loads(r["snapshot"])
        except (TypeError, ValueError):
            snap = None
        valid = (
            isinstance(snap, dict)
            and snap.get("policy") == POLICY
            and snap.get("weights_frozen") is True
            and snap.get("broker_permission") is False
            and snap.get("created_ms") == r["created_ms"]
            and r["created_ms"] > 0
        )
        date = datetime.fromtimestamp(
            r["created_ms"]/1000, tz=timezone.utc
        ).strftime("%Y-%m-%d %H:%M:%S UTC") if (
            type(r["created_ms"]) is int and
            0 < r["created_ms"] < 2**43
        ) else "invalid_timestamp"
        studies.append({
            "run_id": r["id"],
            "created_utc": date,
            "active": r["active"] == 1,
            "frozen_contract_verified": valid,
            "account_scope_sha256_prefix":
                hashlib.sha256(r["context"].encode()).hexdigest()[:16],
            "can_register_new_study": valid and r["active"] == 1,
        })
    return {"available": True, "read_only": True,
            "scope_identity_not_disclosed": True, "studies": studies}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db", type=Path, required=True)
    args = parser.parse_args()
    with sqlite3.connect(args.db.resolve().as_uri()+"?mode=ro",uri=True) as db:
        db.execute("PRAGMA query_only=ON")
        report = list_studies(db)
    print(json.dumps(report, sort_keys=True, ensure_ascii=False, indent=2))


if __name__=="__main__":
    try:
        main()
    except (sqlite3.Error, OSError, ValueError) as exc:
        print(f"STUDY DB NOT AVAILABLE: {exc}",file=sys.stderr)
        sys.exit(2)
