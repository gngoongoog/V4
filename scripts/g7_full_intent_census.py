#!/usr/bin/env python3
"""G7-C read-only population audit: every MT5 open intent during G7 research.

Unlike a win-rate report, the denominator includes *all* observed Core broker
open commands in preregistered account windows, even unmatched/incomplete ones.
This never sends orders, attributes Frozen broker PnL, or promotes a policy.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import json
import sqlite3
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from g7_core_source_audit import check as verify_g7_source
from g7_demo_broker_lineage import audit as verify_demo_labels

MODES = frozenset(("expert", "scalping", "swing"))
MAX_INTENTS = 200_000
VERIFIED = "locally_verified_closed_demo_adaptive"


def _json_object(raw: object) -> dict | None:
    if not isinstance(raw, str):
        return None
    try:
        value = json.loads(raw)
    except (ValueError, TypeError):
        return None
    return value if isinstance(value, dict) else None


def _mode(cmd: dict) -> str | None:
    learning = cmd.get("learning")
    return learning.get("trading_mode") if isinstance(learning, dict) and isinstance(
        learning.get("trading_mode"), str) else (
            cmd.get("trading_mode") if isinstance(cmd.get("trading_mode"), str) else None
        )


def _identity_matches(cmd: dict, row: sqlite3.Row, context: str) -> bool:
    try:
        p = json.loads(context)
    except (ValueError, TypeError):
        return False
    declared = cmd.get("trading_mode")
    learner = cmd.get("learning")
    learned_mode = learner.get("trading_mode") if isinstance(learner, dict) else None
    if isinstance(declared, str) and isinstance(learned_mode, str) and declared != learned_mode:
        return False
    return (isinstance(p, list) and len(p) == 4 and
            cmd.get("id") == row["id"] and
            cmd.get("context") == context and
            cmd.get("kind") == "open" and
            cmd.get("brain") == row["brain"] and
            type(cmd.get("created_ms")) is int and
            cmd["created_ms"] == row["created_ms"] and
            cmd.get("server") == p[0] and cmd.get("login") == p[1] and
            cmd.get("symbol") == p[2] and cmd.get("currency") == p[3])


def audit(db: sqlite3.Connection, *, now_ms: int) -> dict:
    if type(now_ms) is not int or now_ms <= 0:
        raise ValueError("invalid_audit_clock")
    db.row_factory = sqlite3.Row
    source = verify_g7_source(db)
    booked = verify_demo_labels(db, now_ms=now_ms)
    empty = {
        "study": "G7-C", "status": "not_started",
        "registered_runs": source["registered_runs"],
        "paired_decisions": source["paired_decisions"],
        "broker_executed_frozen_control": False,
        "frozen_broker_pnl": None, "paired_broker_delta": None,
        "independent_oos": False, "release_authorized": False,
        "external_broker_attestation": False,
    }
    if source["registered_runs"] == 0:
        return dict(empty, reason="no_new_preregistered_G7_cohort")
    tables = {r[0] for r in db.execute(
        "SELECT name FROM sqlite_master WHERE type='table'")}
    if "broker_commands" not in tables:
        return dict(empty, status="inconclusive", reason="broker_commands_unavailable")
    runs = db.execute(
        "SELECT run_id,context,mode,cohort_start_ms FROM g7_pair_registrations "
        "ORDER BY cohort_start_ms,run_id").fetchall()
    context_start = {}
    mode_start = {}
    for run in runs:
        if run["mode"] not in MODES:
            raise ValueError("invalid_m1_coordinator_mode")
        start = run["cohort_start_ms"]
        if type(start) is not int or start > now_ms:
            raise ValueError("cohort_registration_is_in_future_or_invalid")
        context_start[run["context"]] = min(start, context_start.get(run["context"], start))
        key = (run["context"], run["mode"])
        mode_start[key] = min(start, mode_start.get(key, start))

    pairs = booked.get("rows", [])
    attributed = defaultdict(list)
    for pair in pairs:
        cmdid = pair.get("command_id")
        if cmdid is not None:
            attributed[cmdid].append(pair)

    counts = Counter()
    anomalies = Counter()
    states = Counter()
    entries = []
    matched_ids = set()
    eligible_ids = set()
    all_ids = set()
    for context, first in sorted(context_start.items()):
        total = db.execute(
            "SELECT COUNT(*) FROM broker_commands WHERE context=? AND kind='open' "
            "AND created_ms>=? AND created_ms<=?",
            (context, first, now_ms)).fetchone()[0]
        if total > MAX_INTENTS:
            raise ValueError("too_many_intents_in_scope_for_complete_audit")
        rows = db.execute(
            "SELECT id,context,brain,kind,created_ms,state,payload,result "
            "FROM broker_commands WHERE context=? AND kind='open' "
            "AND created_ms>=? AND created_ms<=? ORDER BY created_ms,id",
            (context, first, now_ms)).fetchall()
        if len(rows) != total:
            raise ValueError("command_universe_changed_during_audit")
        for row in rows:
            counts["all_open_intents"] += 1
            states[str(row["state"])] += 1
            ident = row["id"]
            if ident in all_ids:
                raise ValueError("reused_command_id_across_contexts")
            all_ids.add(ident)
            cmd = _json_object(row["payload"])
            classification = ""
            mode = _mode(cmd) if cmd is not None else None
            if cmd is None or not _identity_matches(cmd, row, context):
                classification = "invalid_open_intent_identity"
                anomalies[classification] += 1
            elif row["brain"] != "system":
                classification = "other_brain_open_intent"
                counts[classification] += 1
            elif mode is None:
                classification = "unknown_trading_mode"
                anomalies[classification] += 1
            elif (context, mode) not in mode_start:
                classification = "other_or_unregistered_trading_mode"
                counts[classification] += 1
            elif row["created_ms"] < mode_start[context, mode]:
                classification = "before_mode_cohort_start"
                counts[classification] += 1
            elif cmd.get("target") != "demo":
                classification = "non_demo_target_in_demo_study"
                anomalies[classification] += 1
            else:
                classification = "eligible_system_demo_open"
                eligible_ids.add(ident)
                counts["eligible_system_demo_open_intents"] += 1

            related = attributed.get(ident, [])
            entry = {"command_id": ident, "created_ms":row["created_ms"],
                     "mode":mode, "command_state":row["state"],
                     "classification":classification}
            if related:
                # A single opening command must belong to exactly one
                # prospective G7 pair. Never count a shared receipt twice.
                if len(related) != 1:
                    anomalies["command_attributed_to_multiple_pairs"] += 1
                    entry["link"] = "ambiguous_multiple_pairs"
                elif classification != "eligible_system_demo_open":
                    anomalies["attribution_outside_eligible_denominator"] += 1
                    entry["link"] = "outside_eligible_denominator"
                else:
                    pair = related[0]
                    entry["link"] = pair["state"]
                    entry["run_id"] = pair["run_id"]
                    entry["bar_time"] = pair["bar_time"]
                    matched_ids.add(ident)
                    counts["exactly_linked_open_intents"] += 1
                    if pair["state"] == VERIFIED:
                        counts["fully_closed_and_locally_verified"] += 1
                    elif pair["state"] == "not_executed":
                        counts["matched_not_executed"] += 1
                    else:
                        counts["matched_unverified_or_unclosed"] += 1
            else:
                entry["link"] = "no_exact_g7_decision_link"
                if classification == "eligible_system_demo_open":
                    anomalies["unmatched_eligible_demo_open_intent"] += 1
            entries.append(entry)

    # Detect any recorded claimed command which vanished from the full
    # broker-command universe (or whose identity/window was altered).
    for cmdid in attributed:
        if cmdid not in all_ids:
            anomalies["g7_claimed_command_absent_from_census"] += 1

    # Per-run G7 decision denominator is supplied by the original source, not
    # reconstructed from winners or open command events.
    counts["paired_decisions"] = source["paired_decisions"]
    counts["g7_decisions_without_exact_open_intent"] = sum(
        pair.get("state") == "no_broker_open_intent" for pair in pairs)

    # Independently census all locally accepted SYSTEM broker learner labels
    # closed after cohort start, not just winners that the G7-B join found.
    labels = []
    if {"coordinator_broker_samples", "coordinator_broker_admissions"}.issubset(tables):
        for context, first in sorted(context_start.items()):
            for row in db.execute(
                "SELECT source,position_id,closed_ms,payload FROM coordinator_broker_samples "
                "WHERE context=? AND brain='system' AND closed_ms>=? AND closed_ms<=?",
                (context,first,now_ms)):
                p = _json_object(row["payload"])
                if p is None:
                    anomalies["malformed_system_broker_label"] += 1
                    continue
                if p.get("trading_mode") not in MODES:
                    counts["other_mode_broker_labels"] += 1
                    continue
                counts["post_cohort_system_broker_labels"] += 1
                linked = p.get("open_command_id") in matched_ids
                if not linked:
                    anomalies["post_cohort_broker_label_without_g7_link"] += 1
                labels.append({"position_id":row["position_id"],
                               "open_command_id":p.get("open_command_id"),
                               "linked_to_g7_decision": linked})

    numerator = counts["exactly_linked_open_intents"]
    denominator = counts["eligible_system_demo_open_intents"]
    fully_closed = counts["fully_closed_and_locally_verified"]
    complete = (denominator > 0 and numerator == denominator and
                not anomalies and counts["matched_unverified_or_unclosed"] == 0 and
                not booked.get("corrupt_broker_labels_by_reason"))
    # "complete" means a local command *census* only, not successful A/B
    # inference, no survivorship bias, verified third-party clock, or LIVE.
    return dict(empty,
        status="inconclusive",
        local_command_census_complete=bool(complete),
        all_open_intents_in_registered_accounts=counts["all_open_intents"],
        eligible_system_demo_open_intents=denominator,
        exactly_linked_open_intents=numerator,
        exact_link_fraction=numerator/denominator if denominator else None,
        locally_verified_full_demo_closes=fully_closed,
        full_close_fraction_of_eligible_open_intents=fully_closed/denominator
            if denominator else None,
        matched_unverified_or_unclosed=counts["matched_unverified_or_unclosed"],
        counts=dict(sorted(counts.items())),
        command_states=dict(sorted(states.items())),
        anomalies=dict(sorted(anomalies.items())),
        g7b_rejected_by_reason=booked.get("unverified_by_reason", {}),
        g7b_broker_booked_net_for_verified_closes_only=booked.get("booked_adaptive_net"),
        scope="all account-scoped opens after each context first G7 cohort start; "
              "mode-specific eligibility after first registration in that mode",
        commands=entries, broker_labels=labels,
        note="Missing/unmatched/open/rejected intents remain in the denominator; "
             "verified broker PnL refers to closed Adaptive ONLY. "
             "This is local evidence, not an executed Frozen comparator.")


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--db", type=Path, required=True)
    p.add_argument("--output", type=Path)
    args = p.parse_args()
    if not args.db.is_file():
        p.exit(2, "G7-C SQLite database does not exist\n")
    try:
        with sqlite3.connect(args.db.resolve().as_uri() + "?mode=ro", uri=True) as db:
            db.execute("PRAGMA query_only=ON")
            db.execute("BEGIN")
            result = audit(db, now_ms=int(time.time()*1000))
            db.rollback()
    except (sqlite3.DatabaseError, ValueError, KeyError, TypeError) as exc:
        p.exit(2, f"G7-C FAIL-CLOSED: {exc}\n")
    output = json.dumps(result, ensure_ascii=False, sort_keys=True, indent=2) + "\n"
    if args.output:
        if args.output.exists():
            p.exit(2, "G7-C report already exists; refusing overwrite\n")
        args.output.write_text(output, encoding="utf-8")
    else:
        print(output, end="")


if __name__ == "__main__":
    main()
