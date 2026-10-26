#!/usr/bin/env python3
"""G7-G: exact, read-only pre/post Core SQLite restart continuity checks.

Take a consistent SQLite backup while Core runs (including WAL contents),
repeat after a Core restart, then verify all pre-restart research data was
preserved byte-for-byte. Locally recorded time/hash is NOT broker attestation.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parent))
from g7_core_source_audit import check as check_g7
from g7f_sealed_candidate_audit import audit as check_candidate

REQUIRED_TABLES=(
    "frozen_coordinator_runs","g7_pair_registrations",
    "g7f_sealed_candidates","frozen_coordinator_decisions",
    "g7_paired_decision_evidence","g7f_sealed_decisions",
)
STATIC_COLUMNS={
    "frozen_coordinator_runs":("id","context","mode","created_ms","snapshot"),
    "g7_pair_registrations":("run_id","context","mode","registered_ms",
                             "cohort_start_ms","frozen_snapshot_sha256","protocol"),
    "g7f_sealed_candidates":("run_id","context","mode","sealed_ms",
                             "checkpoint","checkpoint_sha256","protocol"),
    "frozen_coordinator_decisions":("run_id","bar_time","observed_ms","frozen","adaptive"),
    "g7_paired_decision_evidence":("run_id","bar_time","observed_ms","payload","payload_sha256"),
    "g7f_sealed_decisions":("run_id","bar_time","observed_ms","payload","payload_sha256"),
}
PRIMARY_KEYS={
    "frozen_coordinator_runs":("id",),
    "g7_pair_registrations":("run_id",),
    "g7f_sealed_candidates":("run_id",),
    "frozen_coordinator_decisions":("run_id","bar_time"),
    "g7_paired_decision_evidence":("run_id","bar_time"),
    "g7f_sealed_decisions":("run_id","bar_time"),
}


def _tables(db):
    return {r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table'")}


def _snapshot_rows(db,table):
    columns=STATIC_COLUMNS[table]
    keys=PRIMARY_KEYS[table]
    sql="SELECT "+",".join(columns)+" FROM "+table+" ORDER BY "+",".join(keys)
    rows=[dict(zip(columns,record)) for record in db.execute(sql)]
    return {tuple(row[k] for k in keys):row for row in rows}


def verify(before:sqlite3.Connection,after:sqlite3.Connection,*,now_ms:int)->dict:
    for db in (before,after):
        db.row_factory=sqlite3.Row
        missing=set(REQUIRED_TABLES)-_tables(db)
        if missing:
            raise ValueError("g7g_missing_restart_source_tables:"+",".join(sorted(missing)))
        integrity=db.execute("PRAGMA integrity_check").fetchone()[0]
        if integrity!="ok":
            raise ValueError("g7g_sqlite_integrity_failed")
    original_g7=check_g7(before)
    current_g7=check_g7(after)
    original_candidate=check_candidate(before,now_ms=now_ms)
    current_candidate=check_candidate(after,now_ms=now_ms)
    if original_candidate["status"]=="not_started":
        raise ValueError("g7g_no_sealed_candidate_in_original_backup")
    if current_candidate["status"]!="locally_consistent_candidate_only":
        raise ValueError("g7g_after_candidate_not_locally_consistent")
    counts={}
    for table in REQUIRED_TABLES:
        left=_snapshot_rows(before,table)
        right=_snapshot_rows(after,table)
        for key,row in left.items():
            if key not in right:
                raise ValueError(f"g7g_deleted_original_row:{table}:{key!r}")
            if right[key]!=row:
                raise ValueError(f"g7g_mutated_original_row:{table}:{key!r}")
        counts[table]={"before":len(left),"after":len(right),"added":len(right)-len(left)}
    # Historical backfill is forbidden even when a new row carries internally
    # consistent SHA-256. Only bars observed AFTER the earlier snapshot can be
    # treated as new forward research.
    for table in ("frozen_coordinator_decisions",
                  "g7_paired_decision_evidence","g7f_sealed_decisions"):
        old_rows=_snapshot_rows(before,table)
        new_rows=_snapshot_rows(after,table)
        previous_max={}
        for row in old_rows.values():
            previous_max[row["run_id"]]=max(
                previous_max.get(row["run_id"],-1),row["bar_time"])
        for key,row in new_rows.items():
            if key not in old_rows:
                earlier=previous_max.get(row["run_id"])
                if earlier is not None and row["bar_time"]<=earlier:
                    raise ValueError(f"g7g_backfilled_historical_bar:{table}:{key!r}")
    # A restart can introduce newly registered runs, but cannot drop or revise
    # registered run identity or candidate seal.
    before_run_ids={key[0] for key in _snapshot_rows(
        before,"g7f_sealed_candidates")}
    after_run_ids={key[0] for key in _snapshot_rows(
        after,"g7f_sealed_candidates")}
    if not before_run_ids.issubset(after_run_ids):
        raise ValueError("g7g_original_candidate_registration_disappeared")
    return {
        "study":"G7-G","status":"locally_consistent_restart_only",
        "preserved_sealed_run_count":len(before_run_ids),
        "before_g7_pairs":original_g7["paired_decisions"],
        "after_g7_pairs":current_g7["paired_decisions"],
        "before_candidate_pairs":original_candidate["counters"]["matching_candidate_receipts"],
        "after_candidate_pairs":current_candidate["counters"]["matching_candidate_receipts"],
        "tables":counts,"original_rows_immutable":True,
        "no_historical_bar_backfill":True,
        "candidate_executed_broker_trades":0,
        "live_adaptive_sealed":False,
        "independent_oos":False,
        "real_authorized":False,
        "limitations":"Compares two locally produced Core SQLite snapshots. "
        "Cannot independently prove actual Core restart, uninterrupted quotes, "
        "broker fills, external time, model skill or profits."
    }


def _ro(path:Path):
    if not path.is_file():
        raise ValueError("g7g_database_not_found:"+str(path))
    return sqlite3.connect(path.resolve().as_uri()+"?mode=ro",uri=True)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest="operation",required=True)
    snap=sub.add_parser("snapshot")
    snap.add_argument("--db",type=Path,required=True)
    snap.add_argument("--output",type=Path,required=True)
    check=sub.add_parser("verify")
    check.add_argument("--before",type=Path,required=True)
    check.add_argument("--after",type=Path,required=True)
    check.add_argument("--output",type=Path)
    args=parser.parse_args()
    try:
        if args.operation=="snapshot":
            if args.output.exists() or args.db.resolve()==args.output.resolve():
                raise ValueError("g7g_refuse_snapshot_overwrite_or_same_path")
            with _ro(args.db) as source:
                source.execute("PRAGMA query_only=ON")
                with sqlite3.connect(args.output) as target:
                    source.backup(target)
                    target.execute("PRAGMA integrity_check").fetchone()
            print(json.dumps({"snapshot":str(args.output),
                "status":"local_sqlite_backup_only","independent_attestation":False}))
            return
        if args.output and args.output.exists():
            raise ValueError("g7g_report_already_exists")
        if args.before.resolve()==args.after.resolve():
            raise ValueError("g7g_two_distinct_snapshots_required")
        with _ro(args.before) as left, _ro(args.after) as right:
            left.execute("PRAGMA query_only=ON")
            right.execute("PRAGMA query_only=ON")
            left.execute("BEGIN")
            right.execute("BEGIN")
            report=verify(left,right,now_ms=int(time.time()*1000))
        output=json.dumps(report,ensure_ascii=False,sort_keys=True,indent=2)+"\n"
        if args.output:
            args.output.write_text(output,encoding="utf-8")
        else:
            print(output,end="")
    except (ValueError,sqlite3.DatabaseError,OSError) as exc:
        parser.exit(2,f"G7-G FAIL-CLOSED: {exc}\n")


if __name__=="__main__":
    main()
