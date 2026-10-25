#!/usr/bin/env python3
"""G6-D: read-only causal Expert brain-learning sample attribution audit.

Audits whether each recorded adaptive coordinator member's displayed Demo
broker_samples count could have come ONLY from account-scoped, cost/reward-
verified MT5 broker rows closed BEFORE that decision. A local SQLite snapshot
cannot establish historical INSERT time, prove a model was trained, certify
adaptive OOS, or authenticate broker facts against an external broker.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import json
import math
from pathlib import Path
import sqlite3
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import audit_v25_g6a_preregistered_quotes as g6a
from audit_v25_g6b_executed_demo_arm import _verify_booked


def _source(context: str, member: dict) -> str:
    return json.dumps([
        context, "adaptive-member-v1", "expert",
        member["id"], member["timeframe"], member["report_id"],
    ], ensure_ascii=False, separators=(",", ":"))


def _known_receipts(db: sqlite3.Connection, context: str) -> tuple[dict, Counter]:
    rows=db.execute(
        "SELECT source,position_id,context,brain,closed_ms,payload "
        "FROM coordinator_broker_samples WHERE context=? "
        "ORDER BY closed_ms,position_id,source",(context,)
    ).fetchall()
    grouped=defaultdict(list)
    rejected=Counter()
    for row in rows:
        try:
            p=json.loads(row["payload"])
            if (not isinstance(p, dict)
                or p.get("context") != context
                or p.get("brain") != row["brain"]
                or p.get("position_id") != row["position_id"]
                or p.get("closed_ms") != row["closed_ms"]
                or type(row["closed_ms"]) is not int
                or row["closed_ms"] <= 0
                or type(row["position_id"]) is not int
                or row["position_id"] <= 0):
                raise ValueError("invalid_broker_label_row_identity")
            _verify_booked(p)
            grouped[row["source"]].append(row["closed_ms"])
        except (ValueError, TypeError, KeyError, OverflowError) as exc:
            rejected[str(exc)] += 1
    for values in grouped.values():
        values.sort(reverse=True)
    return grouped, rejected


def audit(db: sqlite3.Connection, manifest: dict, *, now_ms: int) -> dict:
    g6a.audit(db, manifest, now_ms=now_ms)
    doc=g6a._validate_manifest(manifest)
    run=g6a._run(db,doc["run_id"])
    exists=db.execute(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master "
        "WHERE type='table' AND name='coordinator_broker_samples')"
    ).fetchone()[0]
    if not exists:
        return {
            "study":"G6-D",
            "status":"no_broker_learner_evidence_table",
            "training_evidence_consistency":None,
            "independent_oos":False,
            "training_cutoff_independently_enforced":False,
            "broker_pnl":None,"model_promotion_allowed":False,
        }
    candidates,rejected_labels=_known_receipts(db,run["context"])
    frozen_start=doc["holdout_start_ms"]
    decisions=db.execute(
        "SELECT bar_time,observed_ms,adaptive "
        "FROM frozen_coordinator_decisions WHERE run_id=? "
        "AND observed_ms>=? AND observed_ms<=? ORDER BY observed_ms,bar_time",
        (doc["run_id"],frozen_start,now_ms),
    ).fetchall()
    invalid=Counter()
    honest=0
    checked_members=0
    first_weights={}
    latest_weights={}
    previous_counts={}
    progress=Counter()
    post_cutoff_credit=0
    reported_positive=0
    for decision in decisions:
        observed=decision["observed_ms"]
        bar=decision["bar_time"]
        if (type(observed) is not int or type(bar) is not int
                or not (bar+60)*1000<=observed<=(bar+140)*1000):
            invalid["decision_not_causal_m1_completion"] += 1
            continue
        try:
            value=json.loads(decision["adaptive"])
        except (TypeError,ValueError):
            invalid["unreadable_adaptive_decision"] += 1
            continue
        coord=value.get("coordination") if isinstance(value,dict) else None
        members=coord.get("members") if isinstance(coord,dict) else None
        if not isinstance(members,list):
            invalid["missing_adaptive_member_ledger"] += 1
            continue
        seen=set()
        clean=True
        for member in members:
            if not isinstance(member,dict) or any(
                not isinstance(member.get(field),str) or not member[field]
                for field in ("id","timeframe")
            ) or not isinstance(member.get("report_id"),str):
                invalid["incomplete_member_source_identity"] += 1
                clean=False
                continue
            member_id=member["id"]
            if member_id in seen:
                invalid["duplicate_member_in_decision"] += 1
                clean=False
                continue
            seen.add(member_id)
            count=member.get("broker_samples")
            weight=member.get("weight")
            if (type(count) is not int or count<0
                or type(weight) not in (int,float)
                or not math.isfinite(weight) or weight<=0):
                invalid["unverified_member_training_counter_or_weight"] += 1
                clean=False
                continue
            key=_source(run["context"],member)
            known=candidates.get(key,[])
            # The running Rust learner only reads the 4,000 most recent
            # source rows and ignores closes after now. The full earlier set
            # is a conservative UPPER BOUND, not an exact reconstructed count.
            prior=sum(closed<=observed for closed in known)
            if count>prior:
                invalid["claimed_broker_training_exceeds_prior_verified_receipts"] += 1
                clean=False
                continue
            checked_members+=1
            reported_positive+=int(count>0)
            if key not in first_weights:
                first_weights[key]=float(weight)
            if key in previous_counts and count>previous_counts[key]:
                progress["member_broker_sample_count_increased"] += 1
                if any(frozen_start<closed<=observed for closed in known):
                    post_cutoff_credit+=1
            previous_counts[key]=count
            latest_weights[key]=float(weight)
        if clean:
            honest+=1
    weight_changes=sum(
        not math.isclose(first_weights[k],v,rel_tol=1e-12,abs_tol=1e-12)
        for k,v in latest_weights.items()
    )
    verified_label_count=sum(len(x) for x in candidates.values())
    new_labels=sum(
        closed>frozen_start
        for values in candidates.values() for closed in values
    )
    return {
        "study":"G6-D",
        "run_id":doc["run_id"],
        "manifest_sha256":manifest["manifest_sha256"],
        "status":"causal_member_counter_consistency_checked"
                 if decisions and not invalid
                 else "insufficient_or_unverified_training_provenance",
        "decisions_examined":len(decisions),
        "decision_rows_without_detected_inconsistency":honest,
        "member_claims_checked":checked_members,
        "members_reporting_nonzero_broker_samples":reported_positive,
        "source_bound_verified_broker_labels_in_database":verified_label_count,
        "source_bound_labels_closed_after_holdout":new_labels,
        "member_sample_count_increase_observations":
            progress["member_broker_sample_count_increased"],
        "count_increases_with_post_cutoff_labels_available":post_cutoff_credit,
        "members_with_changed_first_to_last_displayed_weights":weight_changes,
        "invalid_claims_by_reason":dict(sorted(invalid.items())),
        "excluded_broker_labels_by_reason":dict(sorted(rejected_labels.items())),
        "training_evidence_consistency":bool(decisions) and not invalid,
        "training_cutoff_independently_enforced":False,
        "adaptive_policy_sealed_across_study":False,
        "local_db_insert_time_attested":False,
        "claim_that_weight_change_was_caused_by_learning":False,
        "independent_oos":False,
        "broker_pnl":None,
        "model_promotion_allowed":False,
        "real_trading_ready":False,
        "explanation":"Checks display-claimed per-member broker sample counts "
            "against the complete historical SOURCE+ACCOUNT scoped, "
            "cost/reward verified prior-close ledger. It is only an upper "
            "bound: current SQLite cannot prove when a backfilled row was "
            "INSERTed nor which trainer code executed; changing weights "
            "can also reflect regime changes. Adaptive is not frozen "
            "for independent OOS or broker-vs-broker attribution.",
    }


def main() -> None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db",type=Path,required=True)
    parser.add_argument("--manifest",type=Path,required=True)
    parser.add_argument("--output",type=Path)
    args=parser.parse_args()
    with sqlite3.connect(args.db.resolve().as_uri()+"?mode=ro",uri=True) as db:
        db.execute("PRAGMA query_only=ON")
        manifest=json.loads(args.manifest.read_text(encoding="utf-8"))
        result=audit(db,manifest,now_ms=int(time.time()*1000))
    body=json.dumps(result,ensure_ascii=False,indent=2,sort_keys=True)
    if args.output:
        if args.output.exists():
            raise FileExistsError("refusing_to_overwrite_previous_research_report")
        args.output.write_text(body+"\n",encoding="utf-8")
    print(body)


if __name__=="__main__":
    try:
        main()
    except (ValueError, OSError, sqlite3.Error) as exc:
        print(f"LEARNING EVIDENCE NOT VERIFIED: {exc}",file=sys.stderr)
        sys.exit(2)
