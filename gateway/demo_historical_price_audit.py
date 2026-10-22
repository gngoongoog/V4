"""Opt-in MT5 Demo read-only *indicative* deal-vs-tick execution audit.

A recent historical tick from the same terminal is not an authenticated,
executable broker quote. Neither price parity nor a favorable slippage estimate
proves an order was admissible, filled fairly, or profitable.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import math
from pathlib import Path
import sys
import time

from demo_evidence_export import DemoEvidenceError, identity, positive, finite_cash

MAX_DEALS = 300
MAX_WINDOW_MS = 24 * 60 * 60 * 1000
MAX_TICK_ROWS = 10_000


def field(row, name, fallback=None):
    if isinstance(row, dict):
        return row.get(name, fallback)
    try:
        return row[name]  # MetaTrader5 copy_ticks_range returns numpy structured arrays
    except (IndexError, KeyError, TypeError, ValueError):
        return getattr(row, name, fallback)


def utc_ms(value):
    return datetime.fromtimestamp(value / 1000, tz=timezone.utc)


def capture(mt5, symbol, from_ms, to_ms, max_quote_age_ms=1500, now_ms=None):
    """Read bounded MT5 Demo history and preceding same-symbol historical ticks.

    Only historical reads. A deal's reference quote must be a fresh tick
    observed at or before deal time; do not look ahead for favorable prices.
    """
    symbol = identity(symbol, "Symbol")
    if (type(from_ms) is not int or type(to_ms) is not int
            or from_ms <= 0 or to_ms <= from_ms
            or to_ms - from_ms > MAX_WINDOW_MS):
        raise DemoEvidenceError("UTC research interval must be positive and at most one day")
    if type(max_quote_age_ms) is not int or not 1 <= max_quote_age_ms <= 3000:
        raise DemoEvidenceError("Quote staleness bound must be between 1 and 3000 ms")
    now_ms = int(time.time() * 1000) if now_ms is None else now_ms
    if type(now_ms) is not int or now_ms < to_ms or now_ms <= 0:
        raise DemoEvidenceError("Future or invalid UTC research interval")
    term = mt5.terminal_info()
    account = mt5.account_info()
    mode = getattr(mt5, "ACCOUNT_TRADE_MODE_DEMO", None)
    if (term is None or not getattr(term, "connected", False)
            or account is None or mode is None
            or getattr(account, "trade_mode", None) != mode):
        raise DemoEvidenceError("Only a connected local MT5 Demo terminal is supported")
    before = (identity(getattr(account, "login", ""), "Login"),
              identity(getattr(account, "server", ""), "Server"),
              identity(getattr(account, "currency", ""), "Currency"),
              getattr(account, "trade_mode", None))
    contract = mt5.symbol_info(symbol)
    size = getattr(contract, "trade_tick_size", None) if contract is not None else None
    if not positive(size):
        raise DemoEvidenceError("Broker tick size is missing or invalid")
    if (getattr(mt5, "DEAL_TYPE_BUY", None) is None
            or getattr(mt5, "DEAL_TYPE_SELL", None) is None
            or getattr(mt5, "COPY_TICKS_ALL", None) is None):
        raise DemoEvidenceError("MT5 deal and historical tick constants unavailable")
    deals = mt5.history_deals_get(utc_ms(from_ms), utc_ms(to_ms - 1))
    if deals is None:
        raise DemoEvidenceError("Deal history unavailable; no artificial zero-deal result")
    if len(deals) > MAX_DEALS:
        raise DemoEvidenceError("Too many deals; use a shorter UTC research interval")
    rows = []
    seen = set()
    missing_quotes = 0
    excluded_other_symbol_deals = 0
    for deal in deals:
        if getattr(deal, "symbol", None) != symbol:
            excluded_other_symbol_deals += 1
            continue
        ticket = getattr(deal, "ticket", None)
        pos = getattr(deal, "position_id", None)
        ts = getattr(deal, "time_msc", None)
        if (type(ticket) is not int or ticket <= 0 or type(pos) is not int
                or pos <= 0 or type(ts) is not int
                or not from_ms <= ts < to_ms or ts > now_ms
                or ticket in seen):
            raise DemoEvidenceError("Duplicate or invalid deal ticket, position or UTC time")
        seen.add(ticket)
        actual = getattr(deal, "price", None)
        lots = getattr(deal, "volume", None)
        entry = getattr(deal, "entry", None)
        side_type = getattr(deal, "type", None)
        costs = {k: getattr(deal, k, None) for k in ("profit", "commission", "swap", "fee")}
        if (not positive(actual) or not positive(lots) or lots > 10000
                or not all(finite_cash(v) for v in costs.values())):
            raise DemoEvidenceError("Invalid deal volume, execution price or fee values")
        side = ("buy" if side_type == mt5.DEAL_TYPE_BUY else
                "sell" if side_type == mt5.DEAL_TYPE_SELL else None)
        # INOUT/netting reversals, close-by and nontrading deals require
        # separate lifecycle reconciliation; never force them into slippage.
        supported_entry = entry in (0, 1)
        ref = None
        age = None
        spread_ticks = None
        deviation_ticks = None
        if side and supported_entry:
            ticks = mt5.copy_ticks_range(
                symbol, utc_ms(max(1, ts - max_quote_age_ms)), utc_ms(ts + 1),
                mt5.COPY_TICKS_ALL)
            if ticks is None:
                raise DemoEvidenceError("MT5 historical tick query unavailable; refuse partial audit")
            if len(ticks) > MAX_TICK_ROWS:
                raise DemoEvidenceError("Historical tick array exceeds bounded research limit")
            chosen = None
            chosen_ms = -1
            for tick in ticks:
                tick_ms = field(tick, "time_msc")
                bid = field(tick, "bid")
                ask = field(tick, "ask")
                if (type(tick_ms) is not int and not hasattr(tick_ms, "__int__")):
                    continue
                tick_ms = int(tick_ms)
                if (tick_ms < ts - max_quote_age_ms or tick_ms > ts
                        or not positive(bid) or not positive(ask) or bid > ask):
                    continue
                if tick_ms > chosen_ms:
                    chosen = (float(bid), float(ask))
                    chosen_ms = tick_ms
            if chosen is not None:
                bid, ask = chosen
                ref = ask if side == "buy" else bid
                age = ts - chosen_ms
                spread_ticks = (ask - bid) / size
                deviation_ticks = ((actual - ref) if side == "buy" else (ref - actual)) / size
            else:
                missing_quotes += 1
        else:
            missing_quotes += 1
        if deviation_ticks is not None and (
                not math.isfinite(deviation_ticks) or not math.isfinite(spread_ticks)):
            raise DemoEvidenceError("Derived price deviation is outside supported numeric range")
        rows.append({
            "deal_ticket": ticket, "position_id": pos, "deal_time_ms": ts,
            "broker_deal_side": side,
            "deal_entry_code": entry, "lots": float(lots),
            "execution_price": float(actual),
            "reference_quote_price": ref,
            "reference_quote_age_ms": age,
            "historical_spread_ticks": spread_ticks,
            # Positive deviation means worse execution, negative means better,
            # relative to a local historical snapshot, NOT guaranteed fill slippage.
            "indicative_adverse_deviation_ticks": deviation_ticks,
            "fees_and_commission_booked_on_supplied_deal":
                float(costs["fee"] + costs["commission"]),
            "broker_reported_profit": float(costs["profit"]),
            "broker_reported_swap": float(costs["swap"]),
            "reference_quality": ("historical_tick_unverified" if ref is not None else
                                  "no_matching_prior_tick_or_unsupported_deal"),
            "broker_quote_executable_verified": False
        })
    final_term = mt5.terminal_info()
    final_account = mt5.account_info()
    after = ((str(getattr(final_account, "login", "")),
              str(getattr(final_account, "server", "")),
              str(getattr(final_account, "currency", "")),
              getattr(final_account, "trade_mode", None))
             if final_account is not None else None)
    if final_term is None or not getattr(final_term, "connected", False) or after != before:
        raise DemoEvidenceError("Demo account switched or disconnected during historical audit")
    rows.sort(key=lambda row: (row["deal_time_ms"], row["deal_ticket"]))
    evaluated = [r["indicative_adverse_deviation_ticks"] for r in rows
                 if r["indicative_adverse_deviation_ticks"] is not None]
    return {
        "policy": "read_only_demo_historical_price_audit_v1",
        "source": "locally_observed_mt5_demo_unattested",
        "account": before[0], "server": before[1],
        "currency": before[2], "symbol": symbol,
        "demo": True,
        "window_start_utc_ms": from_ms,
        "window_end_utc_ms_exclusive": to_ms,
        "captured_utc_ms": now_ms,
        "tick_size": float(size),
        "requested_max_quote_age_ms": max_quote_age_ms,
        "deal_count": len(rows),
        "deals_with_valid_prior_quote": len(evaluated),
        "deals_missing_reference": missing_quotes,
        "excluded_other_symbol_deals": excluded_other_symbol_deals,
        "mean_indicative_adverse_deviation_ticks":
            sum(evaluated) / len(evaluated) if evaluated else None,
        "rows": rows,
        "complete_history": False,
        "account_checked_before_after": True,
        "order_send_calls": 0,
        "broker_approval_verified": False,
        "quote_executability_verified": False,
        "profitability_verified": False,
        "warning": "Historical ticks are not authenticated executable quotes; the latest prior tick may lag the actual broker execution feed, and bid/ask differences are indicative only. No order_check/order_calc_profit, broker acceptance evidence, slippage contract, complete deal history or real PnL validation."
    }


def main(argv=None):
    p = argparse.ArgumentParser(description="Read-only Demo deal-vs-historical-tick research")
    p.add_argument("--symbol", required=True)
    p.add_argument("--from-ms", required=True, type=int)
    p.add_argument("--to-ms", required=True, type=int)
    p.add_argument("--max-quote-age-ms", type=int, default=1500)
    p.add_argument("--output", help="Local new output JSON path; cannot overwrite")
    args = p.parse_args(argv)
    try:
        try:
            import MetaTrader5 as mt5
        except ImportError as exc:
            raise DemoEvidenceError("MetaTrader5 Python package unavailable") from exc
        if not mt5.initialize():
            raise DemoEvidenceError("Local MetaTrader5 initialize failed")
        try:
            payload = capture(mt5, args.symbol, args.from_ms, args.to_ms,
                              max_quote_age_ms=args.max_quote_age_ms)
        finally:
            mt5.shutdown()
        raw = json.dumps(payload, indent=2, ensure_ascii=False, allow_nan=False)
        if args.output:
            out = Path(args.output)
            if out.exists():
                raise DemoEvidenceError("Refusing to overwrite an existing price audit")
            with out.open("x", encoding="utf-8") as handle:
                handle.write(raw + "\n")
            print(f"Saved unverified historical Demo price research: {out}", file=sys.stderr)
        else:
            print(raw)
        return 0
    except (DemoEvidenceError, OSError) as exc:
        print(f"Read-only Demo price audit refused: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
