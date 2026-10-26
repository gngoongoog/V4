#!/usr/bin/env python3
"""G7-H operational handoff: one fail-closed, read-only Demo acceptance packet.

Runs G7-A through G7-G audits on one original Core SQLite read transaction.
This emits a redacted summary with explicit blockers; it never connects to
MT5, sends orders, claims independent broker attestation, or approves REAL.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from g7_core_source_audit import check as g7a
from g7_demo_broker_lineage import audit as g7b
from g7_full_intent_census import audit as g7c
from g7_cohort_overlap_gate import gate as g7d
from g7_holdout_report import audit as g7e
from g7f_sealed_candidate_audit import audit as g7f
from g7g_restart_continuity import verify as g7g

PROTOCOL = "g7h-demo-operator-acceptance-packet-v1"


def _seal_scope(candidate, run_id, start):
    return any(
        entry["run_id"] == run_id and entry["sealed_ms"] < start
        and entry["matching_candidate_receipts"] > 0
        for entry in candidate.get("runs", [])
    )


def summarize(db: sqlite3.Connection, *, now_ms: int,
              manifest: dict | None = None,
              previous_db: sqlite3.Connection | None = None) -> dict:
    """Raise on altered or incomplete *source integrity*. Missing real Demo
    results instead become explicit human-readable pending blockers.
    Never expose account logins, commands, tickets, raw signals or token data.
    """
    db.row_factory = sqlite3.Row
    if type(now_ms) is not int or now_ms <= 0:
        raise ValueError("invalid_acceptance_clock")
    core = g7a(db)
    fields = {
        "protocol": PROTOCOL,
        "status": "not_started",
        "release_authorized": False,
        "live_trading_verified": False,
        "offline_audit_only": True,
        "native_windows_mt5_demo_session_verified": False,
        "broker_history_independently_authenticated": False,
        "paired_executed_frozen_control": False,
        "adaptive_policy_sealed": False,
        "oos_superiority_proven": False,
        "training_improved_broker_returns_proven": False,
        "account_identifiers_exported": False,
        "registered_runs": core["registered_runs"],
        "paired_source_decisions": core["paired_decisions"],
    }
    if core["registered_runs"] == 0:
        return dict(fields, blockers=["g7_research_not_registered"],
                    manual_acceptance_required=True,
                    evidence={"g7a": core["status"]})
    # Abort on ANY corrupted upstream source instead of silently producing a
    # partial green report from successful downstream checks.
    epochs = g7d(db, now_ms=now_ms)
    ledger = g7b(db, now_ms=now_ms)
    census = g7c(db, now_ms=now_ms)
    candidate = g7f(db, now_ms=now_ms)
    if epochs["overlap_gate"] != "locally_consistent":
        raise ValueError("g7_d_prospective_epoch_not_consistent")
    blockers = []
    def block(cond, name):
        if cond:
            blockers.append(name)

    block(core["paired_decisions"] == 0,
          "no_future_closed_m1_decision_pairs")
    counts = candidate.get("counters", {})
    block(candidate["status"] != "locally_consistent_candidate_only",
          "no_locally_verified_sealed_candidate")
    block(counts.get("matching_candidate_receipts", 0) == 0,
          "no_verified_future_sealed_candidate_decisions")
    eligible = census.get("eligible_system_demo_open_intents", 0)
    verified = ledger.get("verified_closed_adaptive_positions", 0)
    block(eligible == 0, "no_eligible_system_demo_broker_open_intents")
    block(verified == 0, "no_exactly_reconciled_fully_closed_demo_positions")
    block(bool(census.get("anomalies")), "command_census_has_exclusions_or_anomalies")
    block(bool(ledger.get("unverified_by_reason")),
          "broker_positions_unverified_or_unclosed")
    block(not census.get("local_command_census_complete", False),
          "complete_core_broker_intent_census_not_verified")

    history = None
    if previous_db is None:
        blockers.append("pre_restart_sqlite_snapshot_not_supplied")
    else:
        previous_db.row_factory = sqlite3.Row
        history = g7g(previous_db, db, now_ms=now_ms)
        if history["status"] != "locally_consistent_restart_only":
            raise ValueError("g7_g_restart_integrity_not_consistent")

    report_holdout = None
    if manifest is None:
        blockers.append("prespecified_holdout_manifest_not_supplied")
    else:
        report_holdout = g7e(db, manifest, now_ms=now_ms)
        if not _seal_scope(candidate, manifest["run_id"],
                           manifest["holdout_start_ms"]):
            blockers.append("sealed_candidate_not_verified_for_selected_holdout")
        if report_holdout["status"] != "observational_descriptive_only":
            blockers.append("holdout_not_sufficient_for_descriptive_review")
        blockers.extend("holdout_" + name for name in
                        report_holdout.get("descriptive_qualification_blockers", []))

    # Even zero local blockers cannot prove actual native MT5 availability,
    # independent broker authenticity, or authorization to trade REAL.
    blockers += ["native_mt5_demo_forward_trial_not_independently_confirmed",
                 "executed_frozen_broker_control_absent"]
    blockers = sorted(set(blockers))
    return dict(
        fields,
        status="awaiting_native_mt5_demo_acceptance"
               if (verified > 0 and core["paired_decisions"] > 0)
               else "insufficient_local_demo_evidence",
        local_validation={
            "g7a_source": core["status"],
            "g7d_cohort_overlap": epochs["overlap_gate"],
            "g7f_sealed_candidate": candidate["status"],
            "g7g_restart": history["status"] if history else "not_checked",
            "g7e_holdout": report_holdout["status"] if report_holdout
                            else "not_provided",
        },
        counts={
            "registered_runs": core["registered_runs"],
            "paired_source_decisions": core["paired_decisions"],
            "sealed_candidates": counts.get("registered_candidates", 0),
            "sealed_candidate_forward_receipts": counts.get(
                "matching_candidate_receipts", 0),
            "all_core_account_open_intents": census.get(
                "all_open_intents_in_registered_accounts", 0),
            "eligible_demo_open_intents": eligible,
            "exactly_linked_open_intents": census.get(
                "exactly_linked_open_intents", 0),
            "locally_verified_closed_adaptive_positions": verified,
        },
        coverage={
            "demo_open_intents_with_full_close_fraction":
                census.get("full_close_fraction_of_eligible_open_intents"),
            "exactly_linked_demo_open_intents_fraction":
                census.get("exact_link_fraction"),
            "closed_subset_win_rate_descriptive":
                report_holdout.get("verified_closed_win_rate")
                if report_holdout else None,
            "holdout_wilson_95_descriptive":
                report_holdout.get("verified_closed_win_rate_wilson_95_descriptive")
                if report_holdout else None,
            "holdout_day_bootstrap_net_r_descriptive":
                report_holdout.get("daily_cluster_bootstrap_mean_net_r_95_descriptive")
                if report_holdout else None,
        },
        exclusion_counts=dict(sorted(census.get("anomalies", {}).items())),
        research_revision="G7-H acceptance packet / local Core evidence only",
        blockers=blockers,
        manual_acceptance_required=True,
        note="Requires a real Windows MT5 Demo forward trial and broker-side "
             "statement review. Do not infer that frozen or sealed shadow "
             "policies were executed at the broker. Local SQLite/sha integrity "
             "alone cannot prove genuine training improvement or Live readiness."
    )


def readonly_connection(path: Path):
    if not path.is_file():
        raise ValueError("sqlite_database_not_found")
    return sqlite3.connect(path.resolve().as_uri() + "?mode=ro", uri=True)


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--db", required=True, type=Path,
                   help="Actual Core data_dir/events.sqlite, never a guessed path")
    p.add_argument("--manifest", type=Path,
                   help="G7-E preregistered manifest; omit for pre-trial review")
    p.add_argument("--before", type=Path,
                   help="Optional earlier G7-G snapshot; compare immutability")
    p.add_argument("--output", required=True, type=Path,
                   help="NEW JSON summary path; contains no account identifiers")
    args = p.parse_args()
    paths = [args.db.resolve(), args.output.resolve()]
    if args.before:
        paths.append(args.before.resolve())
    if args.manifest:
        paths.append(args.manifest.resolve())
    if len(paths) != len(set(paths)) or args.output.exists():
        p.exit(2, "G7-H refuses same-path/overwrite on original evidence\n")
    try:
        manifest = None
        if args.manifest:
            manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
        before_db = None
        try:
            with readonly_connection(args.db) as db:
                db.execute("PRAGMA query_only=ON")
                db.execute("BEGIN")
                if args.before:
                    before_db = readonly_connection(args.before)
                    before_db.execute("PRAGMA query_only=ON")
                    before_db.execute("BEGIN")
                result = summarize(db, now_ms=int(time.time()*1000),
                                   manifest=manifest, previous_db=before_db)
                db.rollback()
        finally:
            if before_db is not None:
                before_db.rollback()
                before_db.close()
    except (ValueError, TypeError, KeyError, OSError, sqlite3.DatabaseError,
            json.JSONDecodeError) as exc:
        # Fail-closed, with NO local broker data or account IDs in output.
        p.exit(2, "G7-H source acceptance FAILED: " + str(exc) + "\n")
    args.output.write_text(
        json.dumps(result, ensure_ascii=False, sort_keys=True, indent=2) + "\n",
        encoding="utf-8",
    )
    print(json.dumps({
        "status": result["status"],
        "report": str(args.output),
        "blockers": len(result["blockers"]),
        "real_authorized": False,
    }, ensure_ascii=False))


if __name__ == "__main__":
    main()
