#!/usr/bin/env python3
"""G7-F: fail-closed read-only audit of immutable Candidate M1 replay.

This does NOT imply online Adaptive is frozen. Neither the Candidate nor the
original Frozen baseline executes broker orders; no broker PnL A/B inference.
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
from g7_core_source_audit import check as verify_core

PROTOCOL="g7f-core-sealed-policy-candidate-v1"

def sha(raw):
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()

def obj(raw):
    try:
        value=json.loads(raw)
    except (ValueError,TypeError) as exc:
        raise ValueError("g7f_broken_json") from exc
    if not isinstance(value,dict):
        raise ValueError("g7f_nonobject_json")
    return value

def canon(value):
    return json.dumps(value,sort_keys=True,separators=(",",":"),ensure_ascii=False)

def audit(db:sqlite3.Connection,*,now_ms:int):
    db.row_factory=sqlite3.Row
    if type(now_ms) is not int or now_ms<=0:
        raise ValueError("invalid_g7f_audit_time")
    core=verify_core(db)
    base={
        "study":"G7-F","source":"Core_g7f_sealed_candidate",
        "status":"not_started",
        "frozen_broker_result":None,"candidate_broker_result":None,
        "broker_a_b_delta":None,"online_adaptive_policy_sealed":False,
        "independently_proven_oos":False,"real_authorized":False,
        "model_promotion_allowed":False,
    }
    tables={r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table'")}
    if "g7f_sealed_candidates" not in tables:
        return dict(base,reason="sealed_candidate_core_table_not_installed")
    regs=db.execute(
        "SELECT c.run_id,c.context,c.mode,c.sealed_ms,c.checkpoint,c.checkpoint_sha256,c.protocol,"
        "g.registered_ms,f.active FROM g7f_sealed_candidates c "
        "JOIN g7_pair_registrations g ON g.run_id=c.run_id "
        "JOIN frozen_coordinator_runs f ON f.id=c.run_id "
        "ORDER BY c.context,c.mode,c.run_id").fetchall()
    if not regs:
        return dict(base,reason="no_explicit_candidate_seal_yet")
    if "g7f_sealed_decisions" not in tables:
        raise ValueError("g7f_candidate_receipt_table_missing")
    counts={"registered_candidates":len(regs),"sealed_future_core_pairs":0,
            "matching_candidate_receipts":0,"not_in_candidate_epoch":0}
    for r in regs:
        if (r["protocol"]!=PROTOCOL or type(r["sealed_ms"]) is not int
            or type(r["registered_ms"]) is not int
            or not r["registered_ms"]<r["sealed_ms"]<=now_ms
            or sha(r["checkpoint"])!=r["checkpoint_sha256"]):
            raise ValueError("g7f_seal_missing_modified_or_wrong_clock")
        snap=obj(r["checkpoint"])
        if (snap.get("mode")!=r["mode"] or snap.get("weights_frozen") is not True
            or snap.get("broker_permission") is not False
            or snap.get("created_ms")!=r["sealed_ms"]):
            raise ValueError("g7f_candidate_policy_not_originally_sealed")
        rows=db.execute(
            "SELECT d.bar_time,d.observed_ms,d.adaptive,e.payload,e.payload_sha256,"
            "e.observed_ms AS evidence_observed_ms "
            "FROM frozen_coordinator_decisions d "
            "LEFT JOIN g7f_sealed_decisions e ON d.run_id=e.run_id "
            "AND d.bar_time=e.bar_time "
            "WHERE d.run_id=? ORDER BY d.bar_time",(r["run_id"],)).fetchall()
        for pair in rows:
            bar=pair["bar_time"]
            observed=pair["observed_ms"]
            if type(bar) is not int or type(observed) is not int or observed>now_ms:
                raise ValueError("g7f_invalid_source_pair_clock")
            if bar*1000<=r["sealed_ms"]:
                counts["not_in_candidate_epoch"]+=1
                if pair["payload"] is not None:
                    raise ValueError("g7f_backfilled_preseal_candidate")
                continue
            counts["sealed_future_core_pairs"]+=1
            if pair["payload"] is None:
                raise ValueError("g7f_missing_future_candidate_receipt")
            if (type(pair["evidence_observed_ms"]) is not int
                    or pair["evidence_observed_ms"]!=observed
                    or sha(pair["payload"])!=pair["payload_sha256"]):
                raise ValueError("g7f_changed_or_late_candidate_receipt")
            receipt=obj(pair["payload"])
            signal=receipt.get("sealed_candidate")
            adaptive=receipt.get("online_adaptive")
            if (receipt.get("protocol")!=PROTOCOL
                or receipt.get("run_id")!=r["run_id"]
                or receipt.get("context")!=r["context"]
                or receipt.get("mode")!=r["mode"]
                or receipt.get("sealed_ms")!=r["sealed_ms"]
                or receipt.get("checkpoint_sha256")!=r["checkpoint_sha256"]
                or receipt.get("bar_time")!=bar
                or receipt.get("observed_ms")!=observed
                or receipt.get("candidate_training_effect") is not False
                or receipt.get("candidate_broker_orders") is not False
                or receipt.get("candidate_booked_pnl") is not None
                or receipt.get("online_adaptive_policy_sealed") is not False
                or not isinstance(signal,dict)
                or not isinstance(adaptive,dict)
                or adaptive != obj(pair["adaptive"])
                or sha(canon(signal))!=receipt.get("sealed_candidate_signal_sha256")
                or sha(pair["adaptive"])!=receipt.get("online_adaptive_signal_sha256")):
                raise ValueError("g7f_forged_or_inconsistent_candidate_decision")
            counts["matching_candidate_receipts"]+=1
    # Orphans are invalid even if correctly hashed.
    extra=db.execute(
        "SELECT COUNT(*) FROM g7f_sealed_decisions e "
        "LEFT JOIN frozen_coordinator_decisions d ON d.run_id=e.run_id "
        "AND d.bar_time=e.bar_time WHERE d.run_id IS NULL").fetchone()[0]
    if extra:
        raise ValueError("g7f_orphan_candidate_decision")
    return dict(base,status="locally_consistent_candidate_only",
                counters=counts,source_core_pairs=core["paired_decisions"],
                candidate_snapshot_read_only=True,
                online_adaptive_learning_continues=True,
                note="Verifies local candidate checkpoint and prospective M1 replay lineage. "
                     "Does not freeze the online Adaptive broker policy or certify OOS.")


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--db",type=Path,required=True)
    p.add_argument("--output",type=Path)
    a=p.parse_args()
    if not a.db.is_file():
        p.exit(2,"G7-F Core DB missing\\n")
    try:
        with sqlite3.connect(a.db.resolve().as_uri()+"?mode=ro",uri=True) as db:
            db.execute("PRAGMA query_only=ON")
            db.execute("BEGIN")
            output=audit(db,now_ms=int(time.time()*1000))
            db.rollback()
    except (ValueError,sqlite3.DatabaseError) as exc:
        p.exit(2,f"G7-F FAIL-CLOSED: {exc}\\n")
    data=json.dumps(output,ensure_ascii=False,indent=2,sort_keys=True)+"\\n"
    if a.output:
        if a.output.exists():
            p.exit(2,"Refusing overwrite of G7-F evidence\\n")
        a.output.write_text(data,encoding="utf-8")
    else:
        print(data,end="")

if __name__=="__main__":
    main()
