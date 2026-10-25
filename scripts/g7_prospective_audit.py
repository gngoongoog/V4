#!/usr/bin/env python3
"""G7 offline, read-only prospective study validator. Does NOT connect to MT5.

Input JSON: {"manifest":{...},"decisions":[...],"outcomes":[...]}.
An empty/missing cohort is NOT_STARTED; unproven evidence is never a PASS.
"""
import argparse
import collections
import hashlib
import json
import math
from pathlib import Path

ARMS = ("Frozen", "Adaptive")
KINDS = ("mt5_demo_execution", "counterfactual_signal", "excluded")

def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)

def digest(value):
    return hashlib.sha256(canonical(value).encode()).hexdigest()

def validate(document):
    if not isinstance(document, dict):
        raise ValueError("study must be a JSON object")
    manifest = document.get("manifest")
    if not isinstance(manifest, dict):
        raise ValueError("missing manifest")
    required = ("protocol", "source_commit", "registered_at_ms", "starts_at_ms",
                "frozen_checkpoint_sha256", "adaptive_checkpoint_sha256",
                "allocation", "minimum_executed_per_arm")
    if any(k not in manifest for k in required) or manifest["protocol"] != "g7-prospective-v1":
        raise ValueError("incomplete/unsupported preregistration")
    if manifest["allocation"] != "sha256-decision-id-parity-v1":
        raise ValueError("unsupported allocation method")
    for field in ("registered_at_ms", "starts_at_ms", "minimum_executed_per_arm"):
        if type(manifest[field]) is not int or manifest[field] < (1 if field == "minimum_executed_per_arm" else 0):
            raise ValueError("invalid " + field)
    if manifest["registered_at_ms"] >= manifest["starts_at_ms"]:
        raise ValueError("preregistration must precede cohort start")
    if not all(isinstance(manifest[k], str) and len(manifest[k]) == 64 and
               all(c in "0123456789abcdef" for c in manifest[k])
               for k in ("frozen_checkpoint_sha256", "adaptive_checkpoint_sha256")):
        raise ValueError("invalid checkpoint hashes")
    decisions = document.get("decisions", [])
    outcomes = document.get("outcomes", [])
    if not isinstance(decisions, list) or not isinstance(outcomes, list):
        raise ValueError("decisions and outcomes must be arrays")
    by_id = {}
    counts = collections.Counter()
    for row in decisions:
        if not isinstance(row, dict):
            raise ValueError("invalid decision")
        uid = row.get("decision_id")
        if not isinstance(uid, str) or not uid or uid in by_id:
            raise ValueError("duplicate/invalid decision id")
        arm = ARMS[int(hashlib.sha256(uid.encode()).hexdigest(), 16) % 2]
        if row.get("assigned_arm") != arm:
            raise ValueError("allocation mismatch: " + uid)
        if type(row.get("decided_at_ms")) is not int or row["decided_at_ms"] < manifest["starts_at_ms"]:
            raise ValueError("nonprospective decision timestamp")
        expected = manifest["frozen_checkpoint_sha256" if arm == "Frozen" else "adaptive_checkpoint_sha256"]
        if row.get("checkpoint_sha256") != expected:
            raise ValueError("checkpoint mismatch: " + uid)
        for k in ("account", "symbol", "mode", "snapshot_sha256"):
            if not isinstance(row.get(k), str) or not row[k]:
                raise ValueError("missing decision identity: " + k)
        by_id[uid] = row
        counts["assigned_" + arm] += 1
    consumed = set()
    booked = {a: [] for a in ARMS}
    for row in outcomes:
        if not isinstance(row, dict):
            raise ValueError("invalid outcome")
        uid = row.get("decision_id")
        if uid not in by_id or uid in consumed:
            raise ValueError("orphan/duplicate outcome")
        consumed.add(uid)
        decision = by_id[uid]
        kind = row.get("kind")
        if kind not in KINDS:
            raise ValueError("unknown outcome kind")
        if kind == "mt5_demo_execution":
            if row.get("broker_account") != decision["account"] or row.get("symbol") != decision["symbol"]:
                raise ValueError("broker identity mismatch")
            if row.get("trade_mode") != "DEMO" or not row.get("position_id"):
                raise ValueError("not verified Demo execution")
            if type(row.get("closed_at_ms")) is not int or row["closed_at_ms"] < decision["decided_at_ms"]:
                raise ValueError("invalid closure clock")
            for k in ("gross", "commission", "swap", "fee", "risk_money", "net", "net_r"):
                if type(row.get(k)) not in (float, int) or not math.isfinite(row[k]):
                    raise ValueError("missing/nonfinite booked field: " + k)
            if row["risk_money"] <= 0:
                raise ValueError("invalid risk")
            calc = row["gross"] + row["commission"] + row["swap"] + row["fee"]
            if abs(calc - row["net"]) > 1e-6 * max(1.0, abs(calc)):
                raise ValueError("booked fee reconciliation failed")
            if abs(row["net"] / row["risk_money"] - row["net_r"]) > 1e-6:
                raise ValueError("booked R reconciliation failed")
            booked[decision["assigned_arm"]].append(row["net_r"])
            counts["booked_" + decision["assigned_arm"]] += 1
        else:
            counts[kind] += 1
    missing = len(by_id) - len(consumed)
    minimum = manifest["minimum_executed_per_arm"]
    status = ("not_started" if not by_id else "inconclusive")
    # Statistical evidence and real broker authenticity are not proven by local JSON alone.
    return {"protocol": "g7-prospective-v1", "status": status, "release_authorized": False,
            "verified_external_broker_provenance": False,
            "manifest_sha256": digest(manifest), "decisions": len(by_id),
            "missing_outcomes": missing, "counts": dict(sorted(counts.items())),
            "minimum_executed_per_arm": minimum,
            "minimum_sample_met": all(len(booked[a]) >= minimum for a in ARMS),
            "booked_mean_r_by_assigned_arm": {a: (sum(booked[a])/len(booked[a]) if booked[a] else None) for a in ARMS},
            "warning": "Locally reconciled claimed DEMO rows are not independently broker authenticated; no causal advantage or REAL readiness established."}

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("study", type=Path)
    p.add_argument("--output", type=Path)
    args = p.parse_args()
    try:
        result = validate(json.loads(args.study.read_text(encoding="utf-8")))
    except (ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        p.exit(2, f"G7 validation failed: {error}\n")
    output = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(output, encoding="utf-8")
    else:
        print(output, end="")

if __name__ == "__main__":
    main()
