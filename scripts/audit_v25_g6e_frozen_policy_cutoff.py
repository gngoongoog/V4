#!/usr/bin/env python3
"""G6-E: retrospective, read-only audit of the FIXED Frozen Expert checkpoint.

Validates the G6-A preregistered snapshot hash, freeze-time policy membership,
per-member broker sample claims against matching cost-verified MT5 Demo closes
BEFORE freeze, and the coverage of post-holdout frozen paper opportunities.
Frozen arm is a FIXED local checkpoint, not a signed independent OOS trial.
Adaptive is still online. Neither paper leg is an MT5 broker fill.
"""
from __future__ import annotations

import argparse
from collections import Counter
import json
import math
from pathlib import Path
import sqlite3
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import audit_v25_g6a_preregistered_quotes as g6a
import audit_v25_g5d_frozen_pairs as g5d
from audit_v25_g6b_executed_demo_arm import _verify_booked


def _source(context: str, *, brain: str, timeframe: str,
            report_id: str) -> str:
    # Mirrors Rust coordinator_learning::source_key / serde JSON tuple.
    return json.dumps([context,"adaptive-member-v1","expert",brain,
                       timeframe,report_id],ensure_ascii=False,
                      separators=(",",":"))


def _valid_broker_before_cutoff(db: sqlite3.Connection, *,
                                source: str, context: str,
                                brain: str, cutoff: int) -> tuple[int,int]:
    """Historical upper bound, NOT evidence of label INSERT time or trainer use."""
    if not db.execute(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master "
        "WHERE type='table' AND name='coordinator_broker_samples')"
    ).fetchone()[0]:
        return 0, 0
    rows = db.execute(
        "SELECT position_id,brain,closed_ms,payload FROM coordinator_broker_samples "
        "WHERE context=? AND source=?", (context,source)
    ).fetchall()
    before=0
    invalid=0
    for r in rows:
        try:
            p=json.loads(r["payload"])
            if (not isinstance(p,dict) or r["brain"]!=brain
                    or p.get("brain")!=brain or p.get("context")!=context
                    or type(r["position_id"]) is not int or r["position_id"]<=0
                    or p.get("position_id")!=r["position_id"]
                    or type(r["closed_ms"]) is not int or r["closed_ms"]<=0
                    or p.get("closed_ms")!=r["closed_ms"]):
                raise ValueError("label_or_member_identity_mismatch")
            _verify_booked(p)
            if r["closed_ms"]<=cutoff:
                before+=1
        except (ValueError,TypeError,OverflowError):
            invalid+=1
    return before,invalid


def audit(db: sqlite3.Connection,manifest: dict,*,now_ms: int) -> dict:
    paper=g6a.audit(db,manifest,now_ms=now_ms)
    doc=g6a._validate_manifest(manifest)
    run=g6a._run(db,doc["run_id"])
    snap=json.loads(run["snapshot"])
    cutoff=run["created_ms"]
    account=json.loads(run["context"])
    if not (isinstance(account,list) and len(account)==4
            and isinstance(account[2],str) and account[2]):
        raise ValueError("unverifiable_frozen_account_scope")
    config=snap.get("config")
    if (not isinstance(config,dict)
            or config.get("symbol")!=account[2]
            or not isinstance(config.get("trading"),dict)
            or config["trading"].get("mode")!="expert"):
        raise ValueError("frozen_config_account_or_mode_mismatch")
    members=snap.get("members")
    if not isinstance(members,list) or not members:
        raise ValueError("frozen_member_snapshot_not_captured")
    failures=Counter()
    counted=0
    verified_prior=0
    unverifiable_prior=0
    positive_claims=0
    seen=set()
    has_system=False
    for item in members:
        if not isinstance(item,dict) or not isinstance(item.get("brain"),dict):
            failures["invalid_frozen_member"]+=1
            continue
        brain=item["brain"]
        mid=brain.get("id")
        timeframe=brain.get("timeframe")
        report_id=brain.get("report_id")
        if (not isinstance(mid,str) or not mid
            or not isinstance(timeframe,str) or not timeframe
            or not isinstance(report_id,str)
            or mid in seen or type(item.get("advisor_enabled")) is not bool
            or "genome" not in item or "weight" not in item):
            failures["invalid_or_duplicate_frozen_member_identity"]+=1
            continue
        seen.add(mid)
        if mid=="system":
            has_system=True
            if item["advisor_enabled"]:
                failures["system_cannot_be_advisor"]+=1
            continue
        if brain.get("enabled") is True and item["advisor_enabled"]:
            failures["solo_member_cannot_be_frozen_advisor"]+=1
            continue
        weight=item["weight"]
        # An absent saved weight can represent an untrained advisor. It must
        # never be counted as a verified frozen learning checkpoint.
        if weight is None:
            continue
        if not isinstance(weight,dict):
            failures["invalid_frozen_weight"]+=1
            continue
        sample_count=weight.get("broker_samples",0)
        displayed=weight.get("weight")
        # Weight() is a Rust computed method, not necessarily serialized as a
        # member field. Never pretend the stored struct proves that value.
        if type(sample_count) is not int or sample_count<0:
            failures["invalid_frozen_broker_sample_count"]+=1
            continue
        if displayed is not None and (type(displayed) not in (int,float)
                or not math.isfinite(displayed)):
            failures["invalid_frozen_weight_fields"]+=1
            continue
        counted+=1
        historical,invalid=_valid_broker_before_cutoff(db,
            source=_source(run["context"],brain=mid,timeframe=timeframe,
                           report_id=report_id),
            context=run["context"],brain=mid,cutoff=cutoff)
        unverifiable_prior+=invalid
        verified_prior+=historical
        if sample_count>0:
            positive_claims+=1
        if sample_count>historical:
            failures["frozen_broker_sample_claim_exceeds_prior_verified_closes"]+=1
    if not has_system:
        failures["system_policy_missing_from_frozen_snapshot"]+=1

    decisions=db.execute(
        "SELECT bar_time,observed_ms,frozen,adaptive FROM "
        "frozen_coordinator_decisions WHERE run_id=? AND observed_ms>=? "
        "AND observed_ms<=? ORDER BY bar_time",
        (doc["run_id"],doc["holdout_start_ms"],now_ms)
    ).fetchall()
    ready=0
    invalid_decisions=0
    for decision in decisions:
        try:
            frozen=json.loads(decision["frozen"])
            if (not isinstance(frozen,dict)
                or type(decision["bar_time"]) is not int
                or type(decision["observed_ms"]) is not int
                or not (decision["bar_time"]+60)*1000<=decision["observed_ms"]
                    <=(decision["bar_time"]+140)*1000):
                raise ValueError("bad_frozen_decision")
            if frozen.get("ready") is True and frozen.get("side") in (-1,1):
                ready+=1
        except (ValueError,TypeError):
            invalid_decisions+=1
    if invalid_decisions:
        failures["invalid_frozen_decision_geometry"]+=invalid_decisions
    valid_pairs=paper["eligible_post_holdout_pairs"]
    return {
        "study":"G6-E",
        "run_id":doc["run_id"],
        "manifest_sha256":manifest["manifest_sha256"],
        "snapshot_sha256":doc["frozen_snapshot_sha256"],
        "freeze_time_training_cutoff_ms":cutoff,
        "freeze_before_registration":cutoff<doc["registered_ms"],
        "frozen_members_in_snapshot":len(members),
        "frozen_members_with_saved_weight":counted,
        "members_claiming_pre_freeze_broker_learning":positive_claims,
        "matching_verified_broker_receipts_closed_before_freeze":verified_prior,
        "invalid_or_legacy_broker_receipts":unverifiable_prior,
        "snapshot_violations":dict(sorted(failures.items())),
        "local_frozen_snapshot_integrity_consistent":not failures,
        "broker_training_claims_exist":positive_claims>0,
        "broker_training_claims_bounded_by_prior_closes":
            positive_claims>0 and not failures,
        "post_holdout_frozen_decisions_examined":len(decisions),
        "post_holdout_frozen_ready_signals":ready,
        "post_holdout_valid_matched_quote_paper_pairs":valid_pairs,
        "frozen_paper_coverage_denominator":"recorded_frozen_ready_signals",
        "unpaired_or_nonqualified_frozen_ready_opportunities":
            max(0,ready-valid_pairs),
        "study_completeness_qualified":bool(decisions) and not failures and
            paper["status"]=="prospective_quote_paper_descriptive_only",
        "status":"fixed_policy_quote_paper_cutoff_consistent" if
            bool(decisions) and not failures
            else "insufficient_or_invalid_fixed_policy_cutoff_evidence",
        "historical_broker_sample_insert_time_proven":False,
        "external_registration_timestamp_verified":False,
        "fixed_frozen_policy_does_not_retrain_during_rust_observe":True,
        "adaptive_policy_training_cutoff_enforced":False,
        "independent_oos":False,
        "paired_real_broker_execution":False,
        "frozen_broker_pnl":None,
        "adaptive_broker_pnl":None,
        "model_promotion_allowed":False,
        "live_execution_authorized":False,
        "caveat":"Frozen weights/genomes/config are read from captured immutable "
            "snapshot in Rust, not updated during Frozen observe. This audit "
            "checks label upper bounds by booked MT5 close time; the original "
            "INSERT time, backfilled training history, and model genome training "
            "provenance cannot be independently proven from this SQLite. "
            "Quote paper misses some opportunities and is not an MT5 fill. "
            "Adaptive comparator keeps learning. No independent OOS claim."
    }


def main() -> None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db",type=Path,required=True)
    parser.add_argument("--manifest",type=Path,required=True)
    parser.add_argument("--output",type=Path)
    args=parser.parse_args()
    with sqlite3.connect(args.db.resolve().as_uri()+"?mode=ro",uri=True) as db:
        db.execute("PRAGMA query_only=ON")
        result=audit(db,json.loads(args.manifest.read_text(encoding="utf-8")),
                     now_ms=int(time.time()*1000))
    text=json.dumps(result,ensure_ascii=False,indent=2,sort_keys=True)
    if args.output:
        if args.output.exists():
            raise FileExistsError("refusing_to_overwrite_existing_report")
        args.output.write_text(text+"\n",encoding="utf-8")
    print(text)


if __name__=="__main__":
    try:
        main()
    except (OSError,ValueError,sqlite3.Error) as exc:
        print(f"FROZEN CHECKPOINT NOT VERIFIED: {exc}",file=sys.stderr)
        sys.exit(2)
