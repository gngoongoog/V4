#!/usr/bin/env python3
"""Read-only local G6-G first-admission chronology for Broker Demo learning.

The Core inserts an admission in the SAME SQLite transaction as a new broker
label. This verifies the saved label hash and local observation time and
compares each future Expert member's claimed broker samples with that prior
admission census. Legacy/backfilled labels without admissions are UNKNOWN,
not new legitimate training. Local SQLite can be edited: NOT trusted TSA/OOS.
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

sys.path.insert(0,str(Path(__file__).resolve().parent))
import audit_v25_g6a_preregistered_quotes as g6a
from audit_v25_g6b_executed_demo_arm import _verify_booked


def census(db:sqlite3.Connection,context:str,*,now_ms:int)->tuple[dict,Counter,int]:
    """Return source -> admitted event timestamps and explicit invalidity."""
    exists=db.execute(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master "
        "WHERE type='table' AND name='coordinator_broker_admissions')"
    ).fetchone()[0]
    if not exists:
        return {},Counter({"admission_table_not_present":1}),0
    sql=(
        "SELECT a.source,a.position_id,a.context,a.brain,a.closed_ms,"
        "a.admitted_ms,a.payload_sha256,a.protocol,"
        "s.context AS sample_context,s.brain AS sample_brain,"
        "s.closed_ms AS sample_closed_ms,s.payload AS sample_payload "
        "FROM coordinator_broker_admissions a "
        "LEFT JOIN coordinator_broker_samples s "
        "ON a.source=s.source AND a.position_id=s.position_id "
        "WHERE a.context=? ORDER BY a.admitted_ms,a.position_id,a.source"
    )
    rows=db.execute(sql,(context,)).fetchall()
    out=defaultdict(list)
    rejected=Counter()
    seen=set()
    for r in rows:
        try:
            pos=r["position_id"]
            ts=r["closed_ms"]
            admitted=r["admitted_ms"]
            if (type(pos) is not int or pos<=0
                or type(ts) is not int or ts<=0
                or type(admitted) is not int or not ts<=admitted<=now_ms
                or r["protocol"]!="g6g-runtime-same-transaction-v1"):
                raise ValueError("invalid_local_admission_clock_or_protocol")
            key=(r["source"],pos)
            if key in seen:
                raise ValueError("duplicate_local_admission")
            seen.add(key)
            p_raw=r["sample_payload"]
            if not isinstance(p_raw,str) or not isinstance(r["payload_sha256"],str):
                raise ValueError("orphan_or_missing_hashed_broker_sample")
            if hashlib.sha256(p_raw.encode("utf-8")).hexdigest()!=r["payload_sha256"]:
                raise ValueError("broker_learning_label_changed_after_admission")
            p=json.loads(p_raw)
            if (r["context"]!=context or r["sample_context"]!=context
                or r["sample_brain"]!=r["brain"]
                or r["sample_closed_ms"]!=ts
                or not isinstance(p,dict)
                or p.get("context")!=context or p.get("brain")!=r["brain"]
                or p.get("position_id")!=pos or p.get("closed_ms")!=ts):
                raise ValueError("admission_broker_identity_mismatch")
            source_tuple=json.loads(r["source"])
            if (not isinstance(source_tuple,list) or len(source_tuple)!=6
                or source_tuple[:3]!=[context,"adaptive-member-v1","expert"]
                or source_tuple[3:]!=[p.get("brain"),p.get("timeframe"),
                                      p.get("report_id")]):
                raise ValueError("admission_source_or_mode_mismatch")
            if p.get("trading_mode")!="expert":
                raise ValueError("admission_source_or_mode_mismatch")
            _verify_booked(p)
            out[r["source"]].append(admitted)
        except (ValueError,TypeError,KeyError,OverflowError) as exc:
            rejected[str(exc)] += 1
    for timestamps in out.values():
        timestamps.sort()
    return out,rejected,len(rows)


def audit(db:sqlite3.Connection,manifest:dict,*,now_ms:int)->dict:
    # G6-A validates preregistered freeze hash and holdout chronology first.
    g6a.audit(db,manifest,now_ms=now_ms)
    doc=g6a._validate_manifest(manifest)
    run=g6a._run(db,doc["run_id"])
    context=run["context"]
    ledger,invalid,raw_count=census(db,context,now_ms=now_ms)
    decisions=db.execute(
        "SELECT observed_ms,adaptive FROM frozen_coordinator_decisions "
        "WHERE run_id=? AND observed_ms>=? AND observed_ms<=? ORDER BY observed_ms",
        (doc["run_id"],doc["holdout_start_ms"],now_ms)
    ).fetchall()
    checked=0
    overclaims=0
    positive=0
    unknown=0
    unreviewed=Counter()
    for d in decisions:
        try:
            parsed=json.loads(d["adaptive"])
            c=parsed["coordination"]
            members=c["members"]
            if not isinstance(members,list):
                raise ValueError("missing_member_list")
            for m in members:
                if (not isinstance(m,dict)
                    or type(m.get("broker_samples")) is not int
                    or m["broker_samples"]<0
                    or not all(isinstance(m.get(k),str)
                               for k in ("id","timeframe","report_id"))):
                    raise ValueError("invalid_member_training_claim")
                source=json.dumps([context,"adaptive-member-v1","expert",
                    m["id"],m["timeframe"],m["report_id"]],
                    ensure_ascii=False,separators=(",",":"))
                observed=d["observed_ms"]
                if type(observed) is not int:
                    raise ValueError("invalid_decision_timestamp")
                admitted=sum(ts<=observed for ts in ledger.get(source,[]))
                claimed=m["broker_samples"]
                checked+=1
                positive+=int(claimed>0)
                if claimed>admitted:
                    # Legacy labels could have been read by old Rust code:
                    # lack of admission is UNSUBSTANTIATED, not time travel proof.
                    overclaims+=1
                    unknown+=claimed-admitted
        except (ValueError,TypeError,KeyError,IndexError) as exc:
            unreviewed[str(exc)]+=1
    table_present="admission_table_not_present" not in invalid
    consistent=(table_present and len(decisions)>0 and checked>0
                and not invalid and not unreviewed and overclaims==0)
    return {
        "study":"G6-G",
        "run_id":doc["run_id"],
        "manifest_sha256":manifest["manifest_sha256"],
        "status":"local_first_admission_consistency_checked" if consistent
             else "unverified_or_incomplete_local_admission_lineage",
        "decisions_checked":len(decisions),
        "expert_member_claims_checked":checked,
        "members_with_nonzero_broker_training_claims":positive,
        "admission_rows_in_scope":raw_count,
        "valid_hashed_expert_admissions_in_scope":sum(map(len,ledger.values())),
        "broker_sample_claims_exceeding_prior_local_admissions":overclaims,
        "unattested_or_legacy_claimed_sample_mass":unknown,
        "rejected_admissions_by_reason":dict(sorted(invalid.items())),
        "unreviewed_decisions_by_reason":dict(sorted(unreviewed.items())),
        "local_admission_consistency":consistent,
        "same_transaction_admission_protocol":
            "g6g-runtime-same-transaction-v1",
        "external_trusted_timestamp":False,
        "db_tampering_or_clock_spoofing_cryptographically_prevented":False,
        "independent_oos":False,
        "adaptive_policy_sealed":False,
        "paired_executed_broker_comparator":False,
        "broker_pnl":None,
        "model_promotion_allowed":False,
        "live_execution_authorized":False,
        "interpretation":"New records include Core's same-transaction first-admitted "
          "clock and a SHA256 of the raw learner payload. This is a local "
          "chronological integrity check, not an external independent time "
          "attestation. Pre-upgrade receipts cannot be retroactively "
          "proved and remain unknown rather than counted as valid admissions."
    }


def main()->None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db",required=True,type=Path)
    parser.add_argument("--manifest",required=True,type=Path)
    parser.add_argument("--output",type=Path)
    args=parser.parse_args()
    with sqlite3.connect(args.db.resolve().as_uri()+"?mode=ro",uri=True) as db:
        db.row_factory=sqlite3.Row
        db.execute("PRAGMA query_only=ON")
        result=audit(db,json.loads(args.manifest.read_text(encoding="utf-8")),
                     now_ms=int(time.time()*1000))
    text=json.dumps(result,ensure_ascii=False,indent=2,sort_keys=True)
    if args.output:
        if args.output.exists():
            raise FileExistsError("refusing_to_overwrite_previous_report")
        args.output.write_text(text+"\n",encoding="utf-8")
    print(text)

if __name__=="__main__":
    try:
        main()
    except (OSError,ValueError,sqlite3.Error) as exc:
        print(f"BROKER ADMISSION CLOCK UNVERIFIED: {exc}",file=sys.stderr)
        sys.exit(2)
