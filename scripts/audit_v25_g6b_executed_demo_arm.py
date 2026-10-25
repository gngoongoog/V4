#!/usr/bin/env python3
"""G6-B: read-only executed Adaptive-arm MT5 Demo evidence for a G6-A study.

Only exactly joined predeclared Expert adaptive signals -> immutable open
broker command -> verified FULLY BOOKED broker-learning receipt qualify.
This is NOT paired broker A/B, NOT a broker Frozen result, and NOT independent
OOS. Does not trade, write SQLite or promote models.
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

import audit_v25_g6a_preregistered_quotes as g6a

MAX_COMMAND_AGE_MS = 180_000


def _finite(v: object) -> float:
    if type(v) not in (int, float) or not math.isfinite(v):
        raise ValueError("missing_nonfinite_cost")
    return float(v)


def _equal(a: float, b: float, *, tolerance: float = 1e-6) -> bool:
    return abs(a-b) <= max(tolerance, abs(a)*1e-8, abs(b)*1e-8)


def _verify_booked(p: dict) -> tuple[float, float, float]:
    if p.get("evidence_source") != "mt5_demo_broker":
        raise ValueError("not_a_demo_broker_receipt")
    gross, commission, swap, fee, net, per_lot = (
        _finite(p.get(field)) for field in
        ("gross", "commission", "swap", "fee", "net", "net_per_lot")
    )
    volume = _finite(p.get("closed_volume"))
    risk = _finite(p.get("risk_money"))
    r = _finite(p.get("r_multiple"))
    reward = _finite(p.get("reward"))
    if volume <= 0 or risk < 0 or not -1 <= reward <= 1:
        raise ValueError("invalid_booked_size_risk_reward")
    expected_r = net / risk if risk > 0 else 0.0
    expected_reward = (math.tanh(expected_r) if risk > 0 else
                       0.25 if net > 0 else -0.25 if net < 0 else 0.0)
    if (not _equal(gross+commission+swap+fee, net)
            or not _equal(net/volume, per_lot)
            or not _equal(r, expected_r)
            or abs(reward-expected_reward) > 1e-8):
        raise ValueError("incorrect_broker_cost_or_reward_lineage")
    return net, per_lot, commission+swap+fee


def _verified_position(db: sqlite3.Connection, row: sqlite3.Row,
                       doc: dict, context: str, now_ms: int) -> tuple[float, float, float, int]:
    pos = row["position_id"]
    closed = row["closed_ms"]
    if type(pos) is not int or pos <= 0 or type(closed) is not int or \
       not doc["holdout_start_ms"] < closed <= now_ms:
        raise ValueError("invalid_or_pre_holdout_broker_close")
    try:
        p = json.loads(row["payload"])
    except (TypeError, json.JSONDecodeError):
        raise ValueError("invalid_broker_payload_json") from None
    if not isinstance(p, dict) or p.get("context") != context or \
       p.get("brain") != "system" or p.get("trading_mode") != "expert" or \
       p.get("position_id") != pos or p.get("closed_ms") != closed:
        raise ValueError("broker_account_mode_or_position_mismatch")
    net, per_lot, costs = _verify_booked(p)
    cmd_id = p.get("open_command_id")
    if not isinstance(cmd_id, str) or not cmd_id:
        raise ValueError("no_exact_open_command_link")
    cmd = db.execute(
        "SELECT id,context,brain,kind,created_ms,state,payload,result "
        "FROM broker_commands WHERE id=? AND context=?",
        (cmd_id, context)
    ).fetchone()
    if cmd is None:
        raise ValueError("broker_command_missing_or_other_account")
    created = cmd["created_ms"]
    if (cmd["kind"] != "open" or cmd["brain"] != "system"
            or cmd["state"] not in ("observed", "filled", "partial", "placed")
            or type(created) is not int
            or created < doc["holdout_start_ms"] or created >= closed):
        raise ValueError("not_a_prospective_system_open_command")
    try:
        payload, result = json.loads(cmd["payload"]), json.loads(cmd["result"])
    except (TypeError, json.JSONDecodeError):
        raise ValueError("unreadable_open_command") from None
    if not isinstance(payload, dict) or not isinstance(result, dict) or \
       payload.get("id") != cmd_id or payload.get("brain") != "system":
        raise ValueError("command_identity_mismatch")
    deal = result.get("deal")
    if type(deal) is not int or deal <= 0 or p.get("open_deal") != deal:
        raise ValueError("exact_broker_entry_deal_missing")
    signal = payload.get("signal")
    if not isinstance(signal, dict) or type(signal.get("bar_time")) is not int:
        raise ValueError("no_original_ex_ante_signal")
    mode = payload.get("learning", {}).get("trading_mode")
    if mode is None:
        mode = signal.get("coordination", {}).get("trading_mode")
    if mode != "expert":
        raise ValueError("command_mode_mismatch")
    bar = signal["bar_time"]
    if bar * 1000 < doc["holdout_start_ms"]:
        raise ValueError("open_signal_before_embargo")
    decision = db.execute(
        "SELECT bar_time,observed_ms,adaptive FROM frozen_coordinator_decisions "
        "WHERE run_id=? AND bar_time=?",
        (doc["run_id"], bar)
    ).fetchone()
    if decision is None:
        raise ValueError("missing_prior_frozen_run_adaptive_decision")
    observed = decision["observed_ms"]
    if type(observed) is not int or not (
        doc["holdout_start_ms"] <= observed <= created <= observed+MAX_COMMAND_AGE_MS
    ):
        raise ValueError("broker_command_not_after_premarket_signal")
    try:
        adaptive = json.loads(decision["adaptive"])
    except (TypeError, json.JSONDecodeError):
        raise ValueError("unreadable_prior_adaptive_decision") from None
    if not isinstance(adaptive, dict) or adaptive.get("ready") is not True or \
       type(adaptive.get("side")) is not int or adaptive["side"] not in (-1,1) or \
       signal.get("side") != adaptive["side"] or \
       signal.get("key") != adaptive.get("key") or \
       p.get("executed_side") != adaptive["side"]:
        raise ValueError("booked_trade_cannot_be_matched_to_adaptive_arm")
    return net, per_lot, costs, closed // 86_400_000


def audit(db: sqlite3.Connection, manifest: dict, *, now_ms: int) -> dict:
    # Carry forward G6-A's strict manifest hash, immutable frozen snapshot,
    # baseline watermarks and prospective holdout constraints.
    g6a.audit(db, manifest, now_ms=now_ms)
    doc = g6a._validate_manifest(manifest)
    run = g6a._run(db, doc["run_id"])
    if not all(
        db.execute("SELECT EXISTS(SELECT 1 FROM sqlite_master "
                   "WHERE type='table' AND name=?)", (t,)).fetchone()[0]
        for t in ("broker_commands", "coordinator_broker_samples")
    ):
        return _empty_report(doc, manifest, "broker_tables_unavailable")
    source = db.execute(
        "SELECT source,position_id,closed_ms,payload "
        "FROM coordinator_broker_samples WHERE context=? AND brain='system' "
        "ORDER BY closed_ms,position_id,source", (run["context"],)
    ).fetchall()
    reasons: Counter[str] = Counter()
    counts = Counter(row["position_id"] for row in source)
    accepted: list[tuple[float,float,float,int]] = []
    days = set()
    for row in source:
        if counts[row["position_id"]] > 1:
            reasons["duplicate_broker_position_attribution"] += 1
            continue
        try:
            result = _verified_position(db,row,doc,run["context"],now_ms)
            accepted.append(result)
            days.add(result[3])
        except (ValueError, TypeError, KeyError, OverflowError,
                json.JSONDecodeError) as exc:
            reasons[str(exc)] += 1
    enough = len(accepted) >= 30 and len(days) >= 5
    return {
        "study": "G6-B", "run_id": doc["run_id"],
        "manifest_sha256": manifest["manifest_sha256"],
        "status": "single_executed_adaptive_arm_descriptive_only" if enough
                  else "insufficient_exactly_attributed_mt5_demo_closes",
        "broker_records_examined": len(source),
        "exactly_linked_adaptive_broker_positions": len(accepted),
        "distinct_utc_days": len(days),
        "rejected_by_reason": dict(sorted(reasons.items())),
        "adaptive_executed_demo_booked_net":
            round(sum(x[0] for x in accepted), 8) if accepted else None,
        "adaptive_executed_demo_net_per_lot_mean":
            round(sum(x[1] for x in accepted)/len(accepted), 8)
            if accepted else None,
        "adaptive_executed_demo_costs":
            round(sum(x[2] for x in accepted), 8) if accepted else None,
        "adaptive_executed_demo_win_rate":
            sum(x[0]>0 for x in accepted)/len(accepted) if accepted else None,
        "frozen_executed_broker_pnl": None,
        "paired_executed_broker_delta": None,
        "broker_fills_for_frozen_comparator": 0,
        "independent_oos": False,
        "registration_independently_timestamped": False,
        "automatic_model_promotion": False,
        "live_execution_authorized": False,
        "note": "Only verified booked MT5 DEMO outcomes matched to the original "
                "pre-outcome Adaptive Expert signal and exact broker open deal. "
                "Frozen is paper-only, and adaptive training is not locked. "
                "This is NOT a paired broker A/B edge or an independent OOS study."
    }


def _empty_report(doc: dict, manifest: dict, reason: str) -> dict:
    return {
        "study":"G6-B","status":reason,"run_id":doc["run_id"],
        "manifest_sha256":manifest["manifest_sha256"],
        "exactly_linked_adaptive_broker_positions":0,
        "adaptive_executed_demo_booked_net":None,
        "frozen_executed_broker_pnl":None,"paired_executed_broker_delta":None,
        "independent_oos":False,"automatic_model_promotion":False,
        "live_execution_authorized":False,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db",required=True,type=Path)
    parser.add_argument("--manifest",required=True,type=Path)
    parser.add_argument("--output",type=Path)
    args = parser.parse_args()
    with sqlite3.connect(args.db.resolve().as_uri()+"?mode=ro",uri=True) as db:
        db.execute("PRAGMA query_only=ON")
        result=audit(db,json.loads(args.manifest.read_text(encoding="utf-8")),
                     now_ms=int(time.time()*1000))
    text=json.dumps(result,indent=2,ensure_ascii=False,sort_keys=True)
    if args.output:
        args.output.write_text(text+"\n",encoding="utf-8")
    print(text)


if __name__=="__main__":
    try:
        main()
    except (ValueError,sqlite3.Error,FileNotFoundError) as exc:
        print(f"EXECUTED STUDY NOT VERIFIED: {exc}",file=sys.stderr)
        sys.exit(2)
