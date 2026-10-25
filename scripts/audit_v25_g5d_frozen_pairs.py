#!/usr/bin/env python3
"""G5-D: read-only, fail-closed audit of EXISTING Expert frozen/adaptive MT5
sampled-quote paper pairs. No price feed, broker connection, order send or learning.
Never interpret paper estimates as broker outcomes or authorize model promotion.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
from pathlib import Path
import random
import sqlite3
from typing import Any

POLICY = "frozen-expert-coordinator-decision-v1"
PAPER_POLICY = "frozen-expert-coordinator-paired-paper-v1"
PROVENANCE = "sampled MT5 Demo bid/ask; NOT a broker fill"
GAP_LIMIT_MS = 15_000
MIN_PAIRS = 30
MIN_DAYS = 5


def _num(v: Any, *, positive: bool = False, nonnegative: bool = False) -> float:
    if type(v) not in (float, int) or not math.isfinite(v):
        raise ValueError("missing_or_nonfinite_economics")
    x = float(v)
    if (positive and x <= 0) or (nonnegative and x < 0):
        raise ValueError("invalid_economic_geometry")
    return x


def _ms(v: Any) -> int:
    if type(v) is not int or v <= 0:
        raise ValueError("invalid_timestamp")
    return v


def _near(a: float, b: float) -> bool:
    return abs(a - b) <= max(1e-7, abs(a) * 1e-8, abs(b) * 1e-8)


def _leg(d: dict, p: dict, side: str, lots: float, opened: int, closed: int,
         tick: float, profit_tick: float, loss_tick: float,
         commission_per_lot: float, slippage_ticks: float) -> tuple[float, int]:
    signal = d.get(side)
    leg = p.get(side)
    if not isinstance(signal, dict) or not isinstance(leg, dict):
        raise ValueError("missing_ex_ante_decision")
    if signal.get("ready") is not True or signal.get("side") not in (-1, 1):
        raise ValueError("not_actionable")
    if leg.get("state") != "closed" or leg.get("side") != signal["side"]:
        raise ValueError("leg_decision_mismatch")
    key = signal.get("key")
    if not isinstance(key, str) or not key or key != leg.get("signal_key"):
        raise ValueError("signal_key_mismatch")
    entry = _num(leg.get("entry"), positive=True)
    expected_entry = _num(p.get("entry_ask" if signal["side"] == 1 else "entry_bid"),
                          positive=True)
    if not _near(entry, expected_entry):
        raise ValueError("entry_price_mismatch")
    if _ms(leg.get("last_quote_ms")) < opened:
        raise ValueError("quote_time_regression")
    result = leg.get("result")
    if not isinstance(result, dict) or result.get("provenance") != PROVENANCE:
        raise ValueError("unknown_price_provenance")
    exit_ms = _ms(result.get("exit_ms"))
    if not opened < exit_ms <= closed:
        raise ValueError("future_or_backdated_exit")
    _num(result.get("exit"), positive=True)
    if result.get("reason") not in ("stop", "target", "time"):
        raise ValueError("unknown_exit_reason")
    gap = result.get("max_quote_gap_ms")
    if type(gap) is not int or gap < 0:
        raise ValueError("unknown_quote_coverage")
    if type(result.get("gap_risk")) is not bool or result["gap_risk"] != (gap > GAP_LIMIT_MS):
        raise ValueError("contradictory_gap_flag")
    if gap > GAP_LIMIT_MS:
        raise ValueError("sampled_quote_gap")
    gross = _num(result.get("gross"))
    fee = _num(result.get("commission_assumed"), nonnegative=True)
    net = _num(result.get("net_paper"))
    per_lot = _num(result.get("net_per_lot"))
    calculated_fee = commission_per_lot * lots
    movement = (float(result["exit"]) - entry) * signal["side"]
    adjusted_ticks = movement / tick - 2 * slippage_ticks
    calculated_gross = adjusted_ticks * (
        profit_tick if adjusted_ticks >= 0 else loss_tick
    ) * lots
    if not (all(_near(a, b) for a, b in (
        (fee, calculated_fee), (gross, calculated_gross),
        (net, gross - fee), (per_lot, net / lots)))):
        raise ValueError("paper_cost_or_result_mismatch")
    return per_lot, exit_ms


def _checked(row: sqlite3.Row, decision: sqlite3.Row, *, run_created: int) -> tuple[int, float]:
    if row["state"] != "complete":
        raise ValueError("not_complete")
    p, d = json.loads(row["payload"]), {
        "frozen": json.loads(decision["frozen"]),
        "adaptive": json.loads(decision["adaptive"]),
    }
    if not isinstance(p, dict):
        raise ValueError("invalid_pair")
    bar, decision_ms = _ms(row["bar_time"]), _ms(decision["observed_ms"])
    if decision["bar_time"] != bar:
        raise ValueError("wrong_decision_bar")
    # Bar is epoch seconds, while all observation/entry/exit times are ms.
    if bar * 1000 <= run_created or not (
        (bar + 60) * 1000 <= decision_ms <= (bar + 140) * 1000
    ):
        raise ValueError("not_prospective_decision")
    opened, closed = _ms(row["opened_ms"]), _ms(row["closed_ms"])
    if opened <= run_created or closed <= opened or abs(decision_ms - opened) > 3000:
        raise ValueError("missing_same_cycle_entry_proof")
    if p.get("policy") != PAPER_POLICY or p.get("source") != \
            "MT5 Demo BID/ASK paired prospective quote; no order_send":
        raise ValueError("not_demo_paper_source")
    if p.get("bar_time") != bar or p.get("observed_ms") != opened:
        raise ValueError("pair_scope_or_timestamp_mismatch")
    contract = p.get("indicative_contract")
    if not isinstance(contract, dict) or contract.get("status") != "pass":
        raise ValueError("no_entry_contract_pass")
    lots = _num(p.get("lots"), positive=True)
    if not _near(lots, 0.01):
        raise ValueError("not_frozen_paper_lot")
    tick = _num(p.get("tick"), positive=True)
    profit = _num(p.get("profit_tick"), positive=True)
    loss = _num(p.get("loss_tick"), positive=True)
    commission = _num(p.get("commission_per_lot"), nonnegative=True)
    slippage = _num(p.get("slippage_ticks"), nonnegative=True)
    if _num(p.get("entry_bid"), positive=True) > _num(p.get("entry_ask"), positive=True):
        raise ValueError("crossed_entry_market")
    f, f_exit = _leg(d, p, "frozen", lots, opened, closed, tick, profit,
                      loss, commission, slippage)
    a, a_exit = _leg(d, p, "adaptive", lots, opened, closed, tick, profit,
                      loss, commission, slippage)
    if closed != max(f_exit, a_exit):
        raise ValueError("contradictory_pair_close")
    return opened // 86_400_000, a - f


def _bootstrap_daily_ci(days: dict[int, list[float]], seed: int = 25) -> list[float] | None:
    if len(days) < MIN_DAYS:
        return None
    keys = sorted(days)
    rng = random.Random(seed)
    draws = []
    for _ in range(2000):
        selected = [keys[rng.randrange(len(keys))] for _ in keys]
        values = [x for day in selected for x in days[day]]
        draws.append(sum(values) / len(values))
    draws.sort()
    return [round(draws[49], 8), round(draws[1949], 8)]


def audit(db: sqlite3.Connection, run_id: str) -> dict:
    db.row_factory = sqlite3.Row
    run = db.execute(
        "SELECT id,context,mode,created_ms,snapshot FROM frozen_coordinator_runs WHERE id=?",
        (run_id,),
    ).fetchone()
    if run is None:
        return {"status": "not_found", "study": "G5D", "broker_pnl": None,
                "live_gate_pass": False, "model_promotion_allowed": False}
    snapshot = json.loads(run["snapshot"])
    if not isinstance(snapshot, dict) or run["mode"] != "expert" or \
       snapshot.get("policy") != POLICY or snapshot.get("weights_frozen") is not True or \
       snapshot.get("broker_permission") is not False or \
       snapshot.get("created_ms") != run["created_ms"]:
        return {"status": "invalid_frozen_run", "study": "G5D", "broker_pnl": None,
                "live_gate_pass": False, "model_promotion_allowed": False}
    created = _ms(run["created_ms"])
    pairs = db.execute(
        """SELECT p.run_id,p.bar_time,p.opened_ms,p.closed_ms,p.state,p.payload,
                  d.bar_time AS d_bar_time,d.observed_ms,d.frozen,d.adaptive
           FROM frozen_coordinator_trade_pairs p
           LEFT JOIN frozen_coordinator_decisions d
             ON d.run_id=p.run_id AND d.bar_time=p.bar_time
           WHERE p.run_id=? ORDER BY p.bar_time""", (run_id,),
    ).fetchall()
    rejects = Counter()
    days: dict[int, list[float]] = defaultdict(list)
    for row in pairs:
        if row["observed_ms"] is None:
            rejects["missing_pre_outcome_decision"] += 1
            continue
        try:
            day, delta = _checked(row, row, run_created=created)
            days[day].append(delta)
        except (ValueError, TypeError, KeyError, OverflowError, json.JSONDecodeError) as exc:
            rejects[str(exc)] += 1
    deltas = [x for day in sorted(days) for x in days[day]]
    eligible = len(deltas)
    distinct_days = len(days)
    status = ("insufficient_prospective_pairs" if eligible < MIN_PAIRS
              else "insufficient_independent_days" if distinct_days < MIN_DAYS
              else "exploratory_quote_paper_only")
    scope_hash = hashlib.sha256(run["context"].encode("utf-8")).hexdigest()[:16]
    return {
        "study": "G5D", "status": status, "run_id": run_id, "mode": "expert",
        "account_scope_hash": scope_hash, "snapshot_created_ms": created,
        "total_paper_rows": len(pairs), "eligible_pairs": eligible,
        "distinct_utc_days": distinct_days, "rejected_by_reason": dict(sorted(rejects.items())),
        "minimum_pairs": MIN_PAIRS, "minimum_distinct_days": MIN_DAYS,
        "adaptive_minus_frozen_mean_paper_net_per_lot": (
            round(sum(deltas) / eligible, 8) if eligible else None
        ),
        "adaptive_minus_frozen_daily_block_bootstrap_95pct": (
            _bootstrap_daily_ci(days) if status == "exploratory_quote_paper_only" else None
        ),
        "broker_pnl": None, "broker_win_rate": None, "independent_oos": False,
        "broker_fills_compared": 0, "paper_estimates_only": True,
        "sampled_quotes_can_skip_stop_target": True,
        "live_gate_pass": False, "model_promotion_allowed": False,
        "interpretation": (
            "Prospective single-frozen-run descriptive comparison, not an independent "
            "out-of-sample broker test. Training cutoff and real order execution are "
            "not independently proved. Never use as REAL readiness."
        ),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db", required=True, type=Path, help="Read-only Core SQLite path")
    parser.add_argument("--run-id", required=True, help="Exact frozen_coordinator_runs.id")
    parser.add_argument("--output", type=Path, help="Optional privacy-scoped JSON report")
    args = parser.parse_args()
    db_uri = args.db.resolve().as_uri() + "?mode=ro"
    with sqlite3.connect(db_uri, uri=True) as db:
        report = audit(db, args.run_id)
    result = json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True)
    if args.output:
        args.output.write_text(result + "\n", encoding="utf-8")
    print(result)


if __name__ == "__main__":
    main()
