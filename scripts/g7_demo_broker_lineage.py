#!/usr/bin/env python3
"""G7-B: read-only, exact Core decision -> Demo broker result attribution.

The Frozen arm NEVER executes. This tool joins only the observed Adaptive
system signal to a same-evaluation open command; a locally admitted, fully
closed MT5 Demo position can then supply *single-arm* booked results.
It does NOT create orders, infer Frozen PnL or authorize model promotion.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
from pathlib import Path
import sqlite3
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from g7_core_source_audit import check as verify_pair_source
from audit_v25_g6b_executed_demo_arm import (
    _verify_booked, _verified_full_broker_deals,
)

LABEL_PROTOCOL = "g6g-runtime-same-transaction-v1"
PAIRED_MODES = {"expert", "scalping", "swing"}


def required_tables(db: sqlite3.Connection) -> bool:
    existing = {r[0] for r in db.execute(
        "SELECT name FROM sqlite_master WHERE type='table'")}
    return {"broker_commands", "broker_deals", "coordinator_broker_samples",
            "coordinator_broker_admissions"}.issubset(existing)


def account_identity(context: str) -> list:
    try:
        parts = json.loads(context)
    except (ValueError, TypeError) as exc:
        raise ValueError("invalid_account_context") from exc
    if not (isinstance(parts, list) and len(parts) == 4
            and isinstance(parts[0], str) and parts[0]
            and type(parts[1]) is int and parts[1] > 0
            and all(isinstance(s, str) and s for s in parts[2:])):
        raise ValueError("invalid_account_context")
    return parts


def parse_object(raw, reason):
    try:
        parsed = json.loads(raw)
    except (TypeError, ValueError) as exc:
        raise ValueError(reason) from exc
    if not isinstance(parsed, dict):
        raise ValueError(reason)
    return parsed


def eligible_command(db, context, mode, observed_ms, adaptive, parts):
    candidates = db.execute(
        "SELECT id,context,brain,kind,created_ms,state,payload,result "
        "FROM broker_commands WHERE context=? AND brain='system' "
        "AND kind='open' AND created_ms=? ORDER BY id",
        (context, observed_ms)).fetchall()
    matched = []
    for row in candidates:
        command = parse_object(row["payload"], "corrupt_same_tick_system_command")
        learning = command.get("learning")
        if (command.get("signal") != adaptive):
            continue
        if (command.get("id") != row["id"]
                or command.get("created_ms") != row["created_ms"]
                or command.get("context") != context
                or command.get("brain") != "system"
                or command.get("kind") != "open"
                or command.get("server") != parts[0]
                or command.get("login") != parts[1]
                or command.get("symbol") != parts[2]
                or command.get("currency") != parts[3]
                or command.get("target") != "demo"
                or not isinstance(learning, dict)
                or learning.get("trading_mode") != mode
                or adaptive.get("ready") is not True
                or adaptive.get("bar_time") is None):
            raise ValueError("matched_command_not_verified_demo_identity")
        matched.append((row, command))
    if len(matched) > 1:
        raise ValueError("multiple_open_commands_for_same_decision")
    return matched[0] if matched else None


def verified_label(db, context, mode, cmdrow, command, adaptive,
                   now_ms, *, candidate_samples):
    command_id = cmdrow["id"]
    if cmdrow["state"] not in ("filled", "partial", "placed", "observed"):
        raise ValueError("command_not_broker_acknowledged")
    result = parse_object(cmdrow["result"], "missing_broker_ack_json")
    open_deal = result.get("deal")
    if type(open_deal) is not int or open_deal <= 0:
        raise ValueError("missing_exact_open_broker_deal")
    labels = [(r, p) for r, p in candidate_samples
              if p.get("open_command_id") == command_id]
    if not labels:
        raise ValueError("missing_accepted_closed_broker_label")
    if len(labels) > 1:
        raise ValueError("ambiguous_multiple_broker_position_labels")
    sample, payload = labels[0]
    position_id = sample["position_id"]
    closed_ms = sample["closed_ms"]
    if (type(position_id) is not int or position_id <= 0
            or type(closed_ms) is not int
            or not cmdrow["created_ms"] < closed_ms <= now_ms):
        raise ValueError("invalid_broker_position_or_close_time")
    if (payload.get("context") != context
            or payload.get("brain") != "system"
            or payload.get("trade_owner") != "system"
            or payload.get("trading_mode") != mode
            or payload.get("position_id") != position_id
            or payload.get("closed_ms") != closed_ms
            or payload.get("open_command_id") != command_id
            or payload.get("open_deal") != open_deal
            or payload.get("executed_side") != adaptive.get("side")
            or sample["context"] != context or sample["brain"] != "system"):
        raise ValueError("booked_label_account_mode_or_execution_mismatch")
    _verify_booked(payload)
    if type(payload.get("risk_money")) not in (int, float) or payload["risk_money"] <= 0:
        raise ValueError("missing_positive_booked_stop_risk")
    admit = db.execute(
        "SELECT * FROM coordinator_broker_admissions "
        "WHERE source=? AND position_id=?",
        (sample["source"], position_id)).fetchone()
    if admit is None:
        raise ValueError("no_original_first_core_admission")
    if (admit["protocol"] != LABEL_PROTOCOL
            or admit["source"] != sample["source"]
            or admit["position_id"] != position_id
            or admit["context"] != context or admit["brain"] != "system"
            or admit["closed_ms"] != closed_ms
            or type(admit["admitted_ms"]) is not int
            or not closed_ms <= admit["admitted_ms"] <= now_ms
            or admit["payload_sha256"] != hashlib.sha256(
                sample["payload"].encode("utf-8")).hexdigest()):
        raise ValueError("changed_or_late_admitted_broker_label")
    learning = command["learning"]
    try:
        source_parts = json.loads(sample["source"])
    except (TypeError, ValueError) as exc:
        raise ValueError("invalid_broker_source_key") from exc
    if source_parts != [context, "adaptive-member-v1", mode, "system",
                        learning.get("timeframe"), learning.get("report_id")]:
        raise ValueError("wrong_causal_member_source")
    _verified_full_broker_deals(db, context, position_id, open_deal,
                                closed_ms, payload, cmdrow["created_ms"])
    return {"command_id":command_id, "position_id":position_id,
            "closed_ms":closed_ms, "admitted_ms":admit["admitted_ms"],
            "net":payload["net"], "net_r":payload["r_multiple"],
            "gross":payload["gross"],
            "costs":payload["commission"] + payload["swap"] + payload["fee"]}


def audit(db: sqlite3.Connection, *, now_ms: int) -> dict:
    db.row_factory = sqlite3.Row
    core = verify_pair_source(db)
    base = {
        "study": "G7-B", "status": "not_started",
        "evidence": "local_core_mt5_demo_single_executed_adaptive_arm",
        "paired_executed_ab": False, "frozen_broker_pnl": None,
        "paired_broker_delta": None, "independent_oos": False,
        "externally_authenticated_broker_history": False,
        "release_authorized": False,
    }
    if core["paired_decisions"] == 0:
        return dict(base, reason="no_G7_prospective_pairs_yet",
                    verified_closed_adaptive_positions=0)
    if type(now_ms) is not int or now_ms < 0:
        raise ValueError("invalid_audit_clock")
    if not required_tables(db):
        return dict(base, status="inconclusive", reason="broker_history_tables_unavailable",
                    paired_decisions=core["paired_decisions"],
                    verified_closed_adaptive_positions=0)
    registrations = db.execute(
        "SELECT run_id,context,mode FROM g7_pair_registrations "
        "ORDER BY registered_ms,run_id").fetchall()
    sample_scope: dict[str, list] = {}
    corrupt_label_rows = Counter()
    results = []
    counts = Counter()
    exclusions = Counter()
    for registration in registrations:
        run_id, context, mode = (registration["run_id"],
                                 registration["context"], registration["mode"])
        if mode not in PAIRED_MODES:
            raise ValueError("unexpected_G7_M1_mode")
        parts = account_identity(context)
        if context not in sample_scope:
            parsed_samples = []
            records = db.execute(
                "SELECT source,position_id,context,brain,closed_ms,payload "
                "FROM coordinator_broker_samples WHERE context=? AND brain='system' "
                "ORDER BY closed_ms,position_id,source",
                (context,)).fetchall()
            for sample in records:
                try:
                    payload = parse_object(sample["payload"], "corrupt_broker_label")
                    parsed_samples.append((sample, payload))
                except ValueError as exc:
                    corrupt_label_rows[str(exc)] += 1
            sample_scope[context] = parsed_samples
        decisions = db.execute(
            "SELECT d.bar_time,d.observed_ms,d.adaptive "
            "FROM frozen_coordinator_decisions d WHERE d.run_id=? "
            "ORDER BY d.bar_time", (run_id,)).fetchall()
        for d in decisions:
            adaptive = parse_object(d["adaptive"], "corrupt_adaptive_decision")
            key = {"run_id":run_id, "bar_time":d["bar_time"]}
            if d["observed_ms"] > now_ms:
                raise ValueError("decision_after_audit_clock")
            match = eligible_command(db, context, mode, d["observed_ms"], adaptive, parts)
            if match is None:
                counts["no_exact_broker_open_intent"] += 1
                results.append(dict(key, state="no_broker_open_intent"))
                continue
            cmdrow, command = match
            counts["exact_open_intents"] += 1
            if cmdrow["state"] in ("cancelled", "rejected", "expired"):
                counts["rejected_or_cancelled_intents"] += 1
                results.append(dict(key, state="not_executed", command_id=cmdrow["id"]))
                continue
            try:
                matched = verified_label(
                    db, context, mode, cmdrow, command, adaptive, now_ms,
                    candidate_samples=sample_scope[context])
            except (ValueError, KeyError, TypeError, OverflowError) as exc:
                reason = str(exc)
                exclusions[reason] += 1
                results.append(dict(key, state="unverified_or_unclosed",
                                    command_id=cmdrow["id"], reason=reason))
                continue
            results.append(dict(key, state="locally_verified_closed_demo_adaptive",
                                context=context, **matched))
    # Exclude ALL uses of one physical position across runs (not just the
    # later occurrence), otherwise double-use can inflate proven performance.
    provisional = [r for r in results if r["state"] == "locally_verified_closed_demo_adaptive"]
    multiplicity = Counter((r["context"], r["position_id"]) for r in provisional)
    for row in provisional:
        if multiplicity[row["context"], row["position_id"]] > 1:
            row["state"] = "unverified_duplicate_physical_position"
            row["reason"] = "duplicate_physical_position_attribution"
            exclusions["duplicate_physical_position_attribution"] += 1
    accepted = [r for r in results if r["state"] == "locally_verified_closed_demo_adaptive"]
    counts["verified_closed_adaptive_positions"] = len(accepted)
    counts["pairs"] = len(results)
    counts["unreviewed_corrupt_label_rows"] = sum(corrupt_label_rows.values())
    return dict(base, status="inconclusive", paired_decisions=core["paired_decisions"],
                source_provenance_checked_local_only=True,
                counts=dict(sorted(counts.items())),
                unverified_by_reason=dict(sorted(exclusions.items())),
                corrupt_broker_labels_by_reason=dict(sorted(corrupt_label_rows.items())),
                verified_closed_adaptive_positions=len(accepted),
                booked_adaptive_net=sum(r["net"] for r in accepted) if accepted else None,
                mean_adaptive_net_r=sum(r["net_r"] for r in accepted)/len(accepted)
                    if accepted else None,
                verified_closed_win_rate=sum(r["net"] > 0 for r in accepted)/len(accepted)
                    if accepted else None,
                booked_metric_scope="verified_full_closes_only_not_all_intents",
                rows=results,
                warning="Frozen was NOT executed. Unclosed, cancelled and unmatched intents are counted, never given synthetic profits. Local hashes and deal history are not independent brokerage proof.")


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--db", required=True, type=Path)
    p.add_argument("--output", type=Path)
    args = p.parse_args()
    if not args.db.is_file():
        p.exit(2, "G7 Core database does not exist\n")
    try:
        with sqlite3.connect(args.db.resolve().as_uri() + "?mode=ro", uri=True) as db:
            db.execute("PRAGMA query_only=ON")
            report = audit(db, now_ms=int(time.time() * 1000))
    except (ValueError, sqlite3.DatabaseError) as exc:
        p.exit(2, f"G7-B FAIL-CLOSED: {exc}\n")
    output = json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    if args.output:
        if args.output.exists():
            p.exit(2, "Refusing to overwrite prior G7 report\n")
        args.output.write_text(output, encoding="utf-8")
    else:
        print(output, end="")


if __name__ == "__main__":
    main()
