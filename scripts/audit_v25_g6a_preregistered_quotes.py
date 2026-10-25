#!/usr/bin/env python3
"""G6-A: preregister a single ongoing Expert Frozen/Adaptive quote-paper study.

This is an offline, READ-ONLY SQLite investigator. It does not open MT5, send
orders, mutate weights or authorize REAL. A local preregistration is self-
attested, not a trusted timestamp: an external timestamped anchor is required
for independently verifiable preregistration. Both arms are sampled quote
counterfactuals, NOT two independent MT5 broker executions.
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

import audit_v25_g5d_frozen_pairs as g5d

POLICY = "g6a-registered-prospective-quote-paper-v1"
MIN_EMBARGO_MS = 30 * 60_000
REQUIRED_PAIRS = 30
REQUIRED_DAYS = 5


def _canonical(value: dict) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=False, allow_nan=False).encode("utf-8")


def _hash(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def _positive_ms(value: object) -> int:
    if type(value) is not int or value <= 0:
        raise ValueError("invalid_study_timestamp")
    return value


def _run(db: sqlite3.Connection, run_id: str) -> sqlite3.Row:
    db.row_factory = sqlite3.Row
    row = db.execute(
        "SELECT id,context,mode,created_ms,snapshot,active "
        "FROM frozen_coordinator_runs WHERE id=?", (run_id,)
    ).fetchone()
    if row is None:
        raise ValueError("frozen_run_not_found")
    try:
        snapshot = json.loads(row["snapshot"])
    except (json.JSONDecodeError, TypeError):
        raise ValueError("invalid_frozen_snapshot") from None
    if (not isinstance(snapshot, dict) or row["mode"] != "expert"
            or snapshot.get("policy") != g5d.POLICY
            or snapshot.get("mode") not in (None, "expert")
            or snapshot.get("weights_frozen") is not True
            or snapshot.get("broker_permission") is not False
            or snapshot.get("created_ms") != row["created_ms"]
            or not isinstance(row["context"], str)
            or not row["context"]):
        raise ValueError("unverified_expert_frozen_run")
    return row


def _watermarks(db: sqlite3.Connection, run_id: str) -> tuple[int, int, int, int]:
    decisions = db.execute(
        "SELECT COUNT(*),COALESCE(MAX(observed_ms),0) "
        "FROM frozen_coordinator_decisions WHERE run_id=?", (run_id,)
    ).fetchone()
    pairs = db.execute(
        "SELECT COUNT(*),COALESCE(MAX(opened_ms),0) "
        "FROM frozen_coordinator_trade_pairs WHERE run_id=?", (run_id,)
    ).fetchone()
    return (int(decisions[0]), int(decisions[1]), int(pairs[0]), int(pairs[1]))


def register(db: sqlite3.Connection, run_id: str, *, now_ms: int,
             embargo_ms: int = MIN_EMBARGO_MS) -> dict:
    now_ms = _positive_ms(now_ms)
    if type(embargo_ms) is not int or embargo_ms < MIN_EMBARGO_MS or \
       embargo_ms > 7 * 86_400_000:
        raise ValueError("embargo_out_of_supported_range")
    run = _run(db, run_id)
    if run["active"] != 1 or not (0 < run["created_ms"] < now_ms):
        raise ValueError("run_must_be_frozen_and_active_before_registration")
    decisions, latest_decision, pairs, latest_pair = _watermarks(db, run_id)
    if max(latest_decision, latest_pair) > now_ms:
        raise ValueError("preexisting_future_dated_rows")
    # A matching payload hash is evidence of an unchanged stored policy,
    # NOT an independently trusted timestamp of when this study was planned.
    document = {
        "policy": POLICY,
        "run_id": run_id,
        "mode": "expert",
        "registered_ms": now_ms,
        "embargo_ms": embargo_ms,
        "holdout_start_ms": now_ms + embargo_ms,
        "frozen_created_ms": run["created_ms"],
        "frozen_snapshot_sha256": _hash(run["snapshot"].encode("utf-8")),
        "account_scope_sha256": _hash(run["context"].encode("utf-8")),
        "baseline_decision_count": decisions,
        "baseline_latest_decision_ms": latest_decision,
        "baseline_pair_count": pairs,
        "baseline_latest_pair_ms": latest_pair,
        "minimum_pairs": REQUIRED_PAIRS,
        "minimum_distinct_utc_days": REQUIRED_DAYS,
        "preregistration_anchor": "local_file_self_attested_not_independent",
        "study_is_broker_execution": False,
    }
    return {"manifest": document, "manifest_sha256": _hash(_canonical(document))}


def _validate_manifest(manifest: dict) -> dict:
    if not isinstance(manifest, dict) or set(manifest) != {"manifest", "manifest_sha256"}:
        raise ValueError("invalid_manifest_envelope")
    doc = manifest["manifest"]
    if not isinstance(doc, dict) or set(doc) != {
        "policy", "run_id", "mode", "registered_ms", "embargo_ms",
        "holdout_start_ms", "frozen_created_ms", "frozen_snapshot_sha256",
        "account_scope_sha256", "baseline_decision_count",
        "baseline_latest_decision_ms", "baseline_pair_count",
        "baseline_latest_pair_ms", "minimum_pairs", "minimum_distinct_utc_days",
        "preregistration_anchor", "study_is_broker_execution",
    }:
        raise ValueError("invalid_manifest_fields")
    try:
        digest = _hash(_canonical(doc))
    except (TypeError, ValueError):
        raise ValueError("invalid_manifest_types") from None
    if digest != manifest["manifest_sha256"]:
        raise ValueError("manifest_hash_mismatch")
    if (doc["policy"] != POLICY or doc["mode"] != "expert"
            or not isinstance(doc["run_id"], str) or not doc["run_id"]
            or doc["preregistration_anchor"] !=
            "local_file_self_attested_not_independent"
            or doc["study_is_broker_execution"] is not False
            or type(doc["embargo_ms"]) is not int
            or not MIN_EMBARGO_MS <= doc["embargo_ms"] <= 7 * 86_400_000
            or type(doc["holdout_start_ms"]) is not int
            or doc["holdout_start_ms"] !=
            _positive_ms(doc["registered_ms"]) + doc["embargo_ms"]
            or type(doc["frozen_created_ms"]) is not int
            or doc["frozen_created_ms"] >= doc["registered_ms"]
            or doc["minimum_pairs"] != REQUIRED_PAIRS
            or doc["minimum_distinct_utc_days"] != REQUIRED_DAYS):
        raise ValueError("inconsistent_study_contract")
    for name in ("baseline_decision_count", "baseline_latest_decision_ms",
                 "baseline_pair_count", "baseline_latest_pair_ms"):
        if type(doc[name]) is not int or doc[name] < 0:
            raise ValueError("invalid_preregistration_watermark")
    for name in ("frozen_snapshot_sha256", "account_scope_sha256"):
        d = doc[name]
        if not isinstance(d, str) or len(d) != 64 or any(
            c not in "0123456789abcdef" for c in d
        ):
            raise ValueError("invalid_scope_digest")
    return doc


def audit(db: sqlite3.Connection, manifest: dict, *, now_ms: int) -> dict:
    now_ms = _positive_ms(now_ms)
    doc = _validate_manifest(manifest)
    run = _run(db, doc["run_id"])
    if (run["created_ms"] != doc["frozen_created_ms"]
            or _hash(run["snapshot"].encode("utf-8")) !=
            doc["frozen_snapshot_sha256"]
            or _hash(run["context"].encode("utf-8")) !=
            doc["account_scope_sha256"]):
        raise ValueError("frozen_policy_or_account_scope_changed")
    if now_ms < doc["holdout_start_ms"]:
        raise ValueError("holdout_has_not_started")
    decisions, latest_decision, pairs, latest_pair = _watermarks(db, doc["run_id"])
    if decisions < doc["baseline_decision_count"] or pairs < doc["baseline_pair_count"]:
        raise ValueError("research_rows_removed_after_preregistration")
    if latest_decision < doc["baseline_latest_decision_ms"] or \
       latest_pair < doc["baseline_latest_pair_ms"]:
        raise ValueError("research_watermark_regressed")
    # Unlike G5-D's all-history summary, only decisions AND paired entries
    # created after the preregistered embargo are eligible for G6-A.
    rows = db.execute(
        """SELECT p.run_id,p.bar_time,p.opened_ms,p.closed_ms,p.state,p.payload,
                  d.bar_time AS d_bar_time,d.observed_ms,d.frozen,d.adaptive
           FROM frozen_coordinator_trade_pairs p
           LEFT JOIN frozen_coordinator_decisions d
             ON d.run_id=p.run_id AND d.bar_time=p.bar_time
           WHERE p.run_id=? ORDER BY p.bar_time""",
        (doc["run_id"],),
    ).fetchall()
    rejects: Counter[str] = Counter()
    days: dict[int, list[float]] = defaultdict(list)
    excluded_before_start = 0
    for row in rows:
        if (row["bar_time"] * 1000 < doc["holdout_start_ms"]
                or row["opened_ms"] < doc["holdout_start_ms"]
                or row["observed_ms"] is None
                or row["observed_ms"] < doc["holdout_start_ms"]):
            excluded_before_start += 1
            continue
        try:
            day, delta = g5d._checked(row, row,
                                     run_created=run["created_ms"],
                                     now_ms=now_ms)
            if not math.isfinite(delta):
                raise ValueError("nonfinite_paired_delta")
            days[day].append(delta)
        except (ValueError, TypeError, KeyError, OverflowError,
                json.JSONDecodeError) as exc:
            rejects[str(exc)] += 1
    values = [x for day in sorted(days) for x in days[day]]
    enough = len(values) >= REQUIRED_PAIRS and len(days) >= REQUIRED_DAYS
    return {
        "study": "G6-A",
        "status": "prospective_quote_paper_descriptive_only" if enough
                  else "insufficient_preregistered_future_quote_pairs",
        "run_id": doc["run_id"],
        "manifest_sha256": manifest["manifest_sha256"],
        "account_scope_sha256": doc["account_scope_sha256"],
        "frozen_snapshot_sha256": doc["frozen_snapshot_sha256"],
        "registered_ms": doc["registered_ms"],
        "holdout_start_ms": doc["holdout_start_ms"],
        "excluded_pre_holdout_pairs": excluded_before_start,
        "rejected_post_holdout_pairs": dict(sorted(rejects.items())),
        "eligible_post_holdout_pairs": len(values),
        "eligible_distinct_utc_days": len(days),
        "minimum_pairs": REQUIRED_PAIRS,
        "minimum_days": REQUIRED_DAYS,
        "adaptive_minus_frozen_mean_quote_paper_net_per_lot":
            round(sum(values) / len(values), 8) if values else None,
        "day_block_bootstrap_95pct_quote_paper":
            g5d._bootstrap_daily_ci(days) if enough else None,
        "prospective_time_filter_enforced": True,
        "registration_independently_timestamped": False,
        "adaptive_training_cutoff_independently_locked": False,
        "independent_oos": False,
        "mt5_broker_fills_compared": 0,
        "broker_pnl": None,
        "real_readiness": False,
        "model_promotion_allowed": False,
        "interpretation": "Only paired sampled-price Demo paper counterfactuals. "
            "Local study registration is not a trusted external timestamp; "
            "Adaptive policy keeps learning, hence no independent frozen "
            "out-of-sample broker profitability claim.",
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    register_cmd = sub.add_parser("register", help="Write one new local manifest")
    register_cmd.add_argument("--db", type=Path, required=True)
    register_cmd.add_argument("--run-id", required=True)
    register_cmd.add_argument("--manifest", type=Path, required=True)
    register_cmd.add_argument("--embargo-minutes", type=int, default=30)
    audit_cmd = sub.add_parser("audit", help="Read immutable manifest and Core DB")
    audit_cmd.add_argument("--db", type=Path, required=True)
    audit_cmd.add_argument("--manifest", type=Path, required=True)
    audit_cmd.add_argument("--output", type=Path)
    args = parser.parse_args()
    with sqlite3.connect(args.db.resolve().as_uri() + "?mode=ro", uri=True) as db:
        db.execute("PRAGMA query_only=ON")
        if args.command == "register":
            document = register(db, args.run_id, now_ms=int(time.time()*1000),
                                embargo_ms=args.embargo_minutes * 60_000)
            # Exclusive create: silently overwriting yesterday's plan would
            # let an operator cherry-pick winning periods after the fact.
            with args.manifest.open("x", encoding="utf-8") as stream:
                stream.write(json.dumps(document, indent=2, sort_keys=True,
                                        ensure_ascii=False) + "\n")
            result = {"registered": True,
                      "manifest_sha256": document["manifest_sha256"],
                      "manifest_path": str(args.manifest),
                      "external_timestamp_anchor_required": True,
                      "broker_pnl": None}
        else:
            manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
            result = audit(db, manifest, now_ms=int(time.time()*1000))
            if args.output:
                args.output.write_text(json.dumps(result, indent=2,
                    sort_keys=True, ensure_ascii=False)+"\n", encoding="utf-8")
    print(json.dumps(result, indent=2, sort_keys=True, ensure_ascii=False))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, sqlite3.Error, FileNotFoundError, FileExistsError) as exc:
        print(f"STUDY NOT VERIFIED: {exc}", file=sys.stderr)
        sys.exit(2)
