#!/usr/bin/env python3
"""G7-D: fail-closed preregistered cohort and source attribution collision audit.

This checker is read-only. It detects duplicated bars across frozen runs,
observations extending past a newer same-account/mode registration,
impossible decision clocks, ambiguous same-cycle MT5 open attribution and
multiple system broker labels for the same physical position.

It cannot verify independent timestamps, actual brokerage authenticity or
profitability; Frozen is NOT an executed control arm.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
import sqlite3
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from g7_core_source_audit import check as verify_core

MAX_PAIRS = 200_000


def _obj(raw):
    try:
        value = json.loads(raw)
    except (TypeError, ValueError):
        return None
    return value if isinstance(value, dict) else None


def _tables(db):
    return {r[0] for r in db.execute(
        "SELECT name FROM sqlite_master WHERE type='table'")}


def gate(db: sqlite3.Connection, *, now_ms: int) -> dict:
    if type(now_ms) is not int or now_ms <= 0:
        raise ValueError("invalid_g7d_audit_clock")
    db.row_factory = sqlite3.Row
    core = verify_core(db)
    result = {
        "study": "G7-D",
        "status": "not_started" if core["registered_runs"] == 0 else "inconclusive",
        "registered_runs": core["registered_runs"],
        "paired_decisions": core["paired_decisions"],
        "source_verification": "locally_verified_only",
        "release_authorized": False,
        "independent_oos": False,
        "frozen_executed_broker_control": False,
        "executed_ab_delta": None,
    }
    if core["registered_runs"] == 0:
        return dict(result, overlap_gate="not_started")
    if core["paired_decisions"] > MAX_PAIRS:
        raise ValueError("g7d_too_many_pairs_to_certify")

    groups = defaultdict(list)
    runs = db.execute(
        "SELECT run_id,context,mode,registered_ms,cohort_start_ms "
        "FROM g7_pair_registrations ORDER BY context,mode,registered_ms,run_id"
    ).fetchall()
    for reg in runs:
        if (type(reg["registered_ms"]) is not int
                or type(reg["cohort_start_ms"]) is not int
                or reg["registered_ms"] > now_ms):
            raise ValueError("g7d_registration_clock_outside_audit")
        groups[reg["context"], reg["mode"]].append(reg)

    next_start = {}
    for (context,mode), ordered in groups.items():
        for idx, run in enumerate(ordered):
            successor = ordered[idx+1] if idx+1 < len(ordered) else None
            if successor is not None:
                if successor["registered_ms"] <= run["cohort_start_ms"]:
                    raise ValueError("g7d_ambiguous_same_mode_registration_boundary")
                next_start[run["run_id"]] = successor["registered_ms"]

    evidence = db.execute(
        "SELECT d.run_id,d.bar_time,d.observed_ms,d.adaptive,"
        " g.context,g.mode,g.cohort_start_ms,g.registered_ms"
        " FROM frozen_coordinator_decisions d"
        " JOIN g7_pair_registrations g ON g.run_id=d.run_id"
        " ORDER BY g.context,g.mode,d.bar_time,d.run_id"
    ).fetchall()
    slots = set()
    full_tick_slots = set()
    command_claims = defaultdict(set)
    coverage = Counter()
    broker_present = "broker_commands" in _tables(db)
    for row in evidence:
        ctx,mode = row["context"], row["mode"]
        bar = row["bar_time"]
        observed = row["observed_ms"]
        run = row["run_id"]
        if type(bar) is not int or bar <= 0 or type(observed) is not int:
            raise ValueError("g7d_missing_bar_or_decision_clock")
        closing = (bar + 60) * 1000
        if (bar*1000 <= row["registered_ms"]
                or observed < row["cohort_start_ms"]
                or observed < closing
                or observed - closing >= 81_000
                or observed > now_ms):
            raise ValueError("g7d_nonprospective_or_late_decision")
        if run in next_start and observed >= next_start[run]:
            raise ValueError("g7d_old_study_decision_after_next_registration")
        slot = (ctx,mode,bar)
        if slot in slots:
            raise ValueError("g7d_duplicate_bar_across_same_mode_runs")
        slots.add(slot)
        tick_slot = (ctx,bar,observed)
        if tick_slot in full_tick_slots:
            raise ValueError("g7d_cross_mode_same_cycle_ambiguous")
        full_tick_slots.add(tick_slot)
        coverage["paired_complete_m1_bars"] += 1

        if not broker_present:
            continue
        adaptive = _obj(row["adaptive"])
        if adaptive is None:
            raise ValueError("g7d_corrupt_adaptive_signal")
        # Exact original Core timestamp, whole-signal equality, account and
        # runtime mode. This is a collision check, not broker verification.
        commands = db.execute(
            "SELECT id,brain,kind,context,payload FROM broker_commands "
            "WHERE context=? AND brain='system' AND kind='open' "
            "AND created_ms=? ORDER BY id", (ctx,observed)).fetchall()
        for cmd in commands:
            p = _obj(cmd["payload"])
            if p is None or p.get("signal") != adaptive:
                continue
            learning = p.get("learning")
            learner_mode = learning.get("trading_mode") if isinstance(learning,dict) else None
            declared_mode = p.get("trading_mode", learner_mode)
            if (p.get("id") != cmd["id"]
                    or p.get("context") != ctx
                    or declared_mode != mode
                    or learner_mode != mode
                    or p.get("brain") != "system"
                    or p.get("target") != "demo"):
                continue
            command_claims[ctx,cmd["id"]].add((run,bar))
            coverage["exact_signal_command_candidates"] += 1

    for key,claims in command_claims.items():
        if len(claims) > 1:
            raise ValueError("g7d_same_broker_command_claimed_by_multiple_studies")

    # An old position can close after registration. The cohort census handles
    # this separately and does not convert the old result into G7 performance.
    # Duplicate SYSTEM labels for one physical position are never evidence of
    # two independently executed trades.
    tables = _tables(db)
    duplicate_positions = 0
    if "coordinator_broker_samples" in tables:
        earliest = {ctx:min(r["cohort_start_ms"]
                  for (c,_),runs_ in groups.items() if c == ctx for r in runs_)
                  for ctx,_ in groups.keys()}
        for context,start in earliest.items():
            duplicates = db.execute(
                "SELECT position_id,COUNT(*) AS n FROM coordinator_broker_samples "
                "WHERE context=? AND brain='system' AND closed_ms>=? "
                "GROUP BY position_id HAVING COUNT(*)>1",
                (context,start)).fetchall()
            duplicate_positions += len(duplicates)
    if duplicate_positions:
        raise ValueError("g7d_multiple_system_labels_for_same_physical_position")

    return dict(result, overlap_gate="locally_consistent",
                command_collision_count=0,
                multiple_system_labels_for_one_position=0,
                counts=dict(sorted(coverage.items())),
                note="Sequential, prospective locally persisted study attribution only. "
                     "No paired broker Frozen execution, externally trusted time, "
                     "or basis to authorize REAL.")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--db", required=True, type=Path)
    p.add_argument("--output", type=Path)
    args = p.parse_args()
    if not args.db.is_file():
        p.exit(2, "G7-D database does not exist\\n")
    try:
        with sqlite3.connect(args.db.resolve().as_uri()+"?mode=ro",uri=True) as db:
            db.execute("PRAGMA query_only=ON")
            db.execute("BEGIN")
            result = gate(db,now_ms=int(time.time()*1000))
            db.rollback()
    except (ValueError,sqlite3.DatabaseError) as e:
        p.exit(2, f"G7-D FAIL-CLOSED: {e}\\n")
    out=json.dumps(result,ensure_ascii=False,indent=2,sort_keys=True)+"\\n"
    if args.output:
        if args.output.exists():
            p.exit(2,"G7-D refuses report overwrite\\n")
        args.output.write_text(out,encoding="utf-8")
    else:
        print(out,end="")


if __name__=="__main__":
    main()
