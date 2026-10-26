#!/usr/bin/env python3
"""G7-E: locally preregistered, read-only MT5 Demo holdout descriptive report.

A time-stamped JSON manifest is not an externally trusted preregistration.
Frozen never executes, Adaptive weights may learn during evaluation, and
verified closed-trade profits alone do not prove profitable OOS superiority.
This module NEVER places orders or authorizes REAL/model promotion.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
from pathlib import Path
import random
import re
import sqlite3
import statistics
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from g7_core_source_audit import check as check_source
from g7_cohort_overlap_gate import gate as check_epochs
from g7_demo_broker_lineage import audit as check_booked
from g7_full_intent_census import audit as check_census
from g7f_sealed_candidate_audit import audit as verify_sealed_candidate

PROTOCOL = "g7e-local-holdout-manifest-v1"
MIN_TRADES = 30
MIN_DAYS = 5
MIN_EMBARGO_MS = 60_000
MS_PER_DAY = 86_400_000


def canonical(doc: dict) -> str:
    return json.dumps(doc, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False)


def digest(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def manifest_hash(doc: dict) -> str:
    return digest(canonical({k:v for k,v in doc.items() if k!="manifest_sha256"}))


def _registered(db: sqlite3.Connection, run_id: str):
    db.row_factory = sqlite3.Row
    return db.execute(
        "SELECT * FROM g7_pair_registrations WHERE run_id=?", (run_id,)
    ).fetchone()


def register(db: sqlite3.Connection, *, run_id: str, source_commit: str,
             training_cutoff_ms: int, holdout_start_ms: int,
             holdout_end_ms: int, now_ms: int) -> dict:
    if not (isinstance(source_commit,str) and re.fullmatch(r"[0-9a-f]{40}",source_commit)):
        raise ValueError("source_commit_requires_exact_40_digit_git_sha")
    check_source(db)  # verification must predate registration
    row = _registered(db,run_id)
    if row is None:
        raise ValueError("unknown_g7_run_id")
    if db.execute(
        "SELECT COUNT(*) FROM frozen_coordinator_decisions WHERE run_id=?",
        (run_id,)).fetchone()[0] != 0:
        raise ValueError("cannot_preregister_after_first_study_decision")
    if not (all(type(n) is int and n>0 for n in
                (now_ms,training_cutoff_ms,holdout_start_ms,holdout_end_ms))
            and training_cutoff_ms <= row["registered_ms"] <= now_ms
            and holdout_start_ms >= now_ms + MIN_EMBARGO_MS
            and holdout_end_ms > holdout_start_ms):
        raise ValueError("nonprospective_holdout_manifest_clock")
    doc = {
        "protocol":PROTOCOL,"run_id":run_id,
        "context_sha256":digest(row["context"]),
        "mode":row["mode"],
        "frozen_snapshot_sha256":row["frozen_snapshot_sha256"],
        "source_commit":source_commit,
        "registered_ms":row["registered_ms"],
        "manifest_created_ms":now_ms,
        "training_cutoff_ms":training_cutoff_ms,
        "holdout_start_ms":holdout_start_ms,
        "holdout_end_ms":holdout_end_ms,
        "min_verified_closes":MIN_TRADES,
        "min_distinct_utc_days":MIN_DAYS,
        "no_model_promotion":True,
        "broker_frozen_control_executed":False,
        "adaptive_policy_seal_proven":False,
        "source":"locally_created_not_externally_timestamped"
    }
    doc["manifest_sha256"]=manifest_hash(doc)
    return doc


def validate_manifest(db: sqlite3.Connection, doc: dict, now_ms: int):
    if not isinstance(doc,dict) or doc.get("protocol")!=PROTOCOL:
        raise ValueError("invalid_holdout_protocol")
    if doc.get("manifest_sha256")!=manifest_hash(doc):
        raise ValueError("modified_holdout_manifest_hash")
    if not (isinstance(doc.get("source_commit"),str)
            and re.fullmatch(r"[0-9a-f]{40}",doc["source_commit"])):
        raise ValueError("invalid_source_commit")
    if (type(doc.get("min_verified_closes")) is not int
            or doc["min_verified_closes"]!=MIN_TRADES
            or type(doc.get("min_distinct_utc_days")) is not int
            or doc["min_distinct_utc_days"]!=MIN_DAYS
            or doc.get("no_model_promotion") is not True
            or doc.get("broker_frozen_control_executed") is not False
            or doc.get("adaptive_policy_seal_proven") is not False
            or doc.get("source")!="locally_created_not_externally_timestamped"):
        raise ValueError("invalid_required_holdout_claims")
    row=_registered(db,doc.get("run_id",""))
    if row is None:
        raise ValueError("missing_preregistered_frozen_study")
    if (doc["context_sha256"]!=digest(row["context"])
            or doc["mode"]!=row["mode"]
            or doc["frozen_snapshot_sha256"]!=row["frozen_snapshot_sha256"]
            or doc["registered_ms"]!=row["registered_ms"]):
        raise ValueError("holdout_registration_identity_changed")
    ts = [doc.get(k) for k in
          ("training_cutoff_ms","registered_ms","manifest_created_ms",
           "holdout_start_ms","holdout_end_ms")]
    if (not all(type(n) is int and n>0 for n in ts)
            or not ts[0]<=ts[1]<=ts[2]
            or ts[3]<ts[2]+MIN_EMBARGO_MS
            or ts[4]<=ts[3] or ts[2]>now_ms):
        raise ValueError("nonprospective_or_unbounded_holdout_window")
    first=db.execute(
        "SELECT MIN(observed_ms) FROM frozen_coordinator_decisions WHERE run_id=?",
        (doc["run_id"],)).fetchone()[0]
    if first is not None and first <= doc["manifest_created_ms"]:
        raise ValueError("manifest_created_after_study_started")
    successor=db.execute(
        "SELECT MIN(registered_ms) FROM g7_pair_registrations "
        "WHERE context=? AND mode=? AND run_id<>? AND registered_ms>?",
        (row["context"],row["mode"],doc["run_id"],row["registered_ms"])
    ).fetchone()[0]
    if successor is not None and doc["holdout_end_ms"]>successor:
        raise ValueError("holdout_spans_new_cohort_registration")
    return row


def wilson95(wins: int, total: int):
    if total == 0:
        return None
    z=1.95996398454
    p=wins/total
    scale=1+z*z/total
    mid=(p+z*z/(2*total))/scale
    rad=z*math.sqrt(p*(1-p)/total+z*z/(4*total*total))/scale
    return [max(0,mid-rad),min(1,mid+rad)]


def daily_bootstrap_ci(rows: list[dict], seed_hex: str):
    """Deterministic day-cluster bootstrap, descriptive not causal/OOS."""
    days=defaultdict(list)
    for row in rows:
        days[row["closed_ms"]//MS_PER_DAY].append(row["net_r"])
    if len(days)<MIN_DAYS:
        return None
    groups=list(days.values())
    rng=random.Random(int(seed_hex[:16],16))
    results=[]
    for _ in range(2000):
        selected=[groups[rng.randrange(len(groups))] for _ in groups]
        samples=[x for group in selected for x in group]
        results.append(statistics.mean(samples))
    results.sort()
    return [results[49], results[1949]]


def audit(db: sqlite3.Connection, doc: dict, *, now_ms: int) -> dict:
    db.row_factory=sqlite3.Row
    row=validate_manifest(db,doc,now_ms)
    check_epochs(db,now_ms=now_ms)
    population=check_census(db,now_ms=now_ms)
    paired=check_booked(db,now_ms=now_ms)
    candidate_proof=verify_sealed_candidate(db,now_ms=now_ms)
    start,end=doc["holdout_start_ms"],doc["holdout_end_ms"]
    # The manifest determines the account, mode, run and time before any
    # result is selected. A position closing later is NOT postdated as proof.
    scoped_command_rows=db.execute(
        "SELECT id,created_ms FROM broker_commands WHERE context=? "
        "AND kind='open' AND created_ms>=? AND created_ms<? "
        "ORDER BY created_ms,id",
        (row["context"],start,min(end,now_ms+1))).fetchall()
    crows={x["command_id"]:x for x in population.get("commands",[])}
    all_command_ids={r["id"] for r in scoped_command_rows}
    eligible=[r for r in scoped_command_rows
              if crows.get(r["id"],{}).get("classification")=="eligible_system_demo_open"
              and crows[r["id"]].get("mode")==row["mode"]]
    eligible_ids={r["id"] for r in eligible}
    pairing = [x for x in paired.get("rows",[])
               if x.get("run_id")==doc["run_id"]
               and type(x.get("bar_time")) is int
               and x["bar_time"]*1000>=start
               and x["bar_time"]*1000<end]
    by_command=defaultdict(list)
    for pair in pairing:
        if pair.get("command_id"):
            by_command[pair["command_id"]].append(pair)
    if any(len(v)>1 for v in by_command.values()):
        raise ValueError("duplicate_broker_command_in_holdout")
    # Full booked closes are counted only when broker closure/admission
    # both fall before the prespecified end, preventing late-result cherry-pick.
    accepted=[]
    unresolved=Counter()
    for command in eligible:
        found=by_command.get(command["id"],[])
        if not found:
            unresolved["unmatched_holdout_open_intent"]+=1
            continue
        p=found[0]
        if p.get("state")!="locally_verified_closed_demo_adaptive":
            unresolved[p.get("state","unknown_state")]+=1
            continue
        if not (type(p.get("closed_ms")) is int
                and type(p.get("admitted_ms")) is int
                and p["closed_ms"]<end and p["admitted_ms"]<end):
            unresolved["booked_or_admitted_after_holdout_end"]+=1
            continue
        accepted.append(p)
    # Broker receipts tied to valid study signals but whose command was
    # absent from the eligible cohort are anomalies, never credited winners.
    out_of_scope=sum(
        p.get("state")=="locally_verified_closed_demo_adaptive"
        and p.get("command_id") not in eligible_ids for p in pairing)
    # Count any source-label admissions entering the adaptive account's
    # learning ledger after the declared cutoff. Even if this count is zero,
    # the model is NOT proven sealed (other learning paths may still exist).
    admitted=0
    if "coordinator_broker_admissions" in {x[0] for x in db.execute(
        "SELECT name FROM sqlite_master WHERE type='table'")}:
        admitted=db.execute(
            "SELECT COUNT(*) FROM coordinator_broker_admissions "
            "WHERE context=? AND admitted_ms>? AND admitted_ms<?",
            (row["context"],doc["training_cutoff_ms"],min(end,now_ms+1))
        ).fetchone()[0]
    denominator=len(eligible)
    coverage=len(accepted)/denominator if denominator else None
    wins=sum(p["net"]>0 for p in accepted)
    distinct_days=len({p["closed_ms"]//MS_PER_DAY for p in accepted})
    days_ci=daily_bootstrap_ci(accepted,doc["manifest_sha256"])
    reasons=[]
    if now_ms<end:
        reasons.append("prespecified_holdout_not_finished")
    if len(accepted)<MIN_TRADES:
        reasons.append("too_few_verified_full_demo_closes")
    if distinct_days<MIN_DAYS:
        reasons.append("too_few_distinct_utc_days")
    if denominator==0:
        reasons.append("no_eligible_prospective_open_intents")
    if len(accepted)!=denominator:
        reasons.append("not_all_eligible_intents_fully_closed_and_attributed")
    if population.get("local_command_census_complete") is not True:
        reasons.append("whole_account_command_census_incomplete")
    if admitted:
        reasons.append("new_broker_training_labels_admitted_after_cutoff")
    if out_of_scope:
        reasons.append("out_of_cohort_booked_attribution")
    sufficient=(not reasons)
    # These are deliberately extreme intent-level bounds, NOT the win rate
    # of executed positions: every unresolved command may be nonexecuted,
    # a loss or a win, and no synthetic return is assigned to it.
    intent_bounds=([wins/denominator,
                    (wins+denominator-len(accepted))/denominator]
                   if denominator else None)
    return {
        "study":"G7-E","status":"observational_descriptive_only" if sufficient
                 else "holdout_in_progress" if now_ms<end
                 else "inconclusive",
        "manifest_sha256":doc["manifest_sha256"],"run_id":doc["run_id"],
        "mode":doc["mode"],"training_cutoff_ms":doc["training_cutoff_ms"],
        "holdout_start_ms":start,"holdout_end_ms":end,
        "source_commit":doc["source_commit"],
        "account_context_sha256":doc["context_sha256"],
        "locally_preregistered":True,"external_preregistration_timestamp":False,
        "frozen_policy_checkpoint_sha256":doc["frozen_snapshot_sha256"],
        "adaptive_policy_sealed":False,"adaptive_online_learning_possible":True,
        "sealed_candidate_replay_integrity":candidate_proof["status"],
        "sealed_candidate_is_read_only_shadow":
            candidate_proof["status"]=="locally_consistent_candidate_only",
        "sealed_candidate_broker_execution":False,
        "new_broker_training_labels_admitted_after_cutoff":admitted,
        "all_account_open_intents_in_holdout":len(all_command_ids),
        "eligible_system_demo_open_intents":denominator,
        "matched_verified_fully_closed_demo_positions":len(accepted),
        "unverified_or_unmatched_eligible_open_intents":dict(sorted(unresolved.items())),
        "booked_out_of_cohort_position_claims":out_of_scope,
        "complete_broker_outcome_coverage":coverage,
        "intent_level_win_fraction_extreme_bounds_not_execution_win_rate":intent_bounds,
        "descriptive_qualification_blockers":reasons,
        "verified_closed_win_rate":wins/len(accepted) if accepted else None,
        "verified_closed_win_rate_wilson_95_descriptive":wilson95(wins,len(accepted)),
        "verified_closed_mean_net_r":statistics.mean(p["net_r"] for p in accepted)
                 if accepted else None,
        "daily_cluster_bootstrap_mean_net_r_95_descriptive":days_ci,
        "distinct_utc_days_with_verified_closes":distinct_days,
        "minimum_closed_trades_policy":MIN_TRADES,
        "minimum_distinct_utc_days_policy":MIN_DAYS,
        "sufficient_local_descriptive_sample":sufficient,
        "frozen_broker_pnl":None,"paired_broker_delta":None,
        "independent_out_of_sample":False,"adaptive_vs_frozen_superiority_proven":False,
        "release_authorized":False,"model_promotion_allowed":False,
        "limitations":"Only locally verified Adaptive Demo fully closed positions. "
          "Missing outcomes remain in the intent denominator. Wilson/cluster-bootstrap "
          "intervals describe the selected closed subset, not a causal edge. "
          "Adaptive may train during holdout; no frozen broker control exists. "
          "Local SHA/time is not independent trusted registration."
    }


def main():
    p=argparse.ArgumentParser(description=__doc__)
    sub=p.add_subparsers(dest="operation",required=True)
    create=sub.add_parser("register",help="register BEFORE first study decision")
    create.add_argument("--db",required=True,type=Path)
    create.add_argument("--run-id",required=True)
    create.add_argument("--source-commit",required=True)
    create.add_argument("--training-cutoff-ms",required=True,type=int)
    create.add_argument("--holdout-start-ms",required=True,type=int)
    create.add_argument("--holdout-end-ms",required=True,type=int)
    create.add_argument("--output",required=True,type=Path)
    review=sub.add_parser("audit",help="read-only completed/partial cohort report")
    review.add_argument("--db",required=True,type=Path)
    review.add_argument("--manifest",required=True,type=Path)
    review.add_argument("--output",required=True,type=Path)
    args=p.parse_args()
    if not args.db.is_file():
        p.exit(2,"Core SQLite does not exist\\n")
    if args.output.exists():
        p.exit(2,"Output already exists: refusing to overwrite preregistration evidence\\n")
    try:
        with sqlite3.connect(args.db.resolve().as_uri()+"?mode=ro",uri=True) as db:
            db.row_factory=sqlite3.Row
            db.execute("PRAGMA query_only=ON")
            db.execute("BEGIN")
            now=int(time.time()*1000)
            if args.operation=="register":
                result=register(db,run_id=args.run_id,source_commit=args.source_commit,
                    training_cutoff_ms=args.training_cutoff_ms,
                    holdout_start_ms=args.holdout_start_ms,
                    holdout_end_ms=args.holdout_end_ms,now_ms=now)
            else:
                result=audit(db,json.loads(args.manifest.read_text(encoding="utf-8")),
                             now_ms=now)
            db.rollback()
    except (ValueError,KeyError,TypeError,sqlite3.DatabaseError,OSError) as exc:
        p.exit(2,f"G7-E FAIL-CLOSED: {exc}\\n")
    args.output.write_text(json.dumps(result,ensure_ascii=False,indent=2,sort_keys=True)
                           +"\\n",encoding="utf-8")
    print(json.dumps({"output":str(args.output),"status":result.get("status","locally_registered"),
                      "release_authorized":False},ensure_ascii=False))


if __name__=="__main__":
    main()
