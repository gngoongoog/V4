#!/usr/bin/env python3
"""Read-only MT5 Demo chart acceptance. Never sends or modifies a trade/order.

Run on Windows with MT5 terminal open and the MetaTrader5 Python package:
    py -3 scripts/check_mt5_chart_readonly.py --symbol XAUUSD

Output is intentionally account-ID free. A closed/weekend market can have
valid historical bars but cannot be certified as a fresh live quote feed.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import math
import sys
from typing import Any

FRAMES = (
    ("M1", "TIMEFRAME_M1", 60),
    ("M5", "TIMEFRAME_M5", 300),
    ("M15", "TIMEFRAME_M15", 900),
    ("M30", "TIMEFRAME_M30", 1800),
    ("H1", "TIMEFRAME_H1", 3600),
    ("H4", "TIMEFRAME_H4", 14400),
    ("D1", "TIMEFRAME_D1", 86400),
)


def validate_bars(bars: Any, seconds: int, now_s: int) -> tuple[bool, str, int | None]:
    """Validate actual closed bars, preserving MT5 market/session gaps."""
    if bars is None or len(bars) < 100 or len(bars) > 180:
        return False, "HISTORY_NOT_READY", None
    previous = 0
    try:
        for bar in bars:
            stamp = int(bar["time"])
            op = float(bar["open"])
            hi = float(bar["high"])
            lo = float(bar["low"])
            close = float(bar["close"])
            volume = int(bar["tick_volume"])
            # Daily broker candles can start at non-UTC-midnight: do not
            # wrongly reject a valid D1 session on timezone alignment alone.
            if (stamp <= previous or stamp <= 0 or
                    (seconds <= 3600 and stamp % seconds != 0) or
                    stamp + seconds > now_s + 2):
                return False, "TIMESTAMP_OR_OPEN_BAR", None
            if (not all(math.isfinite(x) and x > 0 for x in (op, hi, lo, close)) or
                    hi < max(op, close) or lo > min(op, close) or hi < lo or
                    volume < 0 or float(bar["tick_volume"]) != volume):
                return False, "INVALID_OHLC_OR_VOLUME", None
            previous = stamp
    except (TypeError, ValueError, IndexError, KeyError, OverflowError):
        return False, "INVALID_BROKER_PAYLOAD", None
    return True, "OK", previous + seconds


def inspect(mt5: Any, symbol: str, now_ms: int) -> dict[str, Any]:
    """Read-only: account/terminal/symbol/tick information and closed bars."""
    result: dict[str, Any] = {
        "schema": "braintrade-chart-demo-readonly-v1",
        "symbol": symbol,
        "execution_performed": False,
        "account_login": "REDACTED",
        "frames": {},
        "status": "UNVERIFIED",
    }
    acct = mt5.account_info()
    terminal = mt5.terminal_info()
    demo_mode = getattr(mt5, "ACCOUNT_TRADE_MODE_DEMO", 0)
    if not acct or getattr(acct, "trade_mode", None) != demo_mode:
        result["status"] = "DEMO_ACCOUNT_REQUIRED"
        return result
    if terminal is None or getattr(terminal, "connected", False) is not True:
        result["status"] = "MT5_TERMINAL_OFFLINE"
        return result
    identity = (
        getattr(acct, "login", None), getattr(acct, "server", None),
        getattr(acct, "trade_mode", None),
    )
    info = mt5.symbol_info(symbol)
    if info is None:
        result["status"] = "EXACT_BROKER_SYMBOL_NOT_FOUND"
        return result
    result["account_type"] = "DEMO"
    quote = mt5.symbol_info_tick(symbol)
    tick_ms = getattr(quote, "time_msc", None)
    age = now_ms - tick_ms if isinstance(tick_ms, int) else None
    result["quote_age_ms"] = age if age is not None and age >= 0 else None
    result["quote_recent"] = bool(age is not None and 0 <= age <= 3000 and
                                  isinstance(getattr(quote, "bid", None), (int, float)) and
                                  isinstance(getattr(quote, "ask", None), (int, float)) and
                                  math.isfinite(quote.bid) and math.isfinite(quote.ask) and
                                  quote.bid > 0 and quote.ask >= quote.bid)
    valid_history = True
    current_history = True
    for label, constant, seconds in FRAMES:
        try:
            frame_id = getattr(mt5, constant)
            rates = mt5.copy_rates_from_pos(symbol, frame_id, 1, 180)
            ok, reason, last_end = validate_bars(rates, seconds, now_ms // 1000)
            n = len(rates) if rates is not None else 0
        except (TypeError, ValueError, RuntimeError, AttributeError, OverflowError):
            ok, reason, last_end, n = False, "READ_ERROR", None, 0
        since = now_ms - last_end * 1000 if last_end is not None else None
        # Distinguish valid historical records from current feed behavior.
        recent = bool(ok and since is not None and 0 <= since <= max(300, 2 * seconds) * 1000)
        result["frames"][label] = {
            "history_integrity": "PASS" if ok else "FAIL",
            "reason": reason, "closed_bars": n,
            "last_closed_utc": datetime.fromtimestamp(last_end, timezone.utc).isoformat()
                               if last_end is not None else None,
            "last_close_age_s": round(since / 1000) if since is not None else None,
            "recent_history": recent,
            "source": "MT5_COPY_RATES_FROM_POS_CLOSED",
        }
        valid_history &= ok
        current_history &= recent
    acct_after = mt5.account_info()
    account_stable = bool(acct_after and identity == (
        getattr(acct_after, "login", None), getattr(acct_after, "server", None),
        getattr(acct_after, "trade_mode", None),
    ))
    result["account_unchanged"] = account_stable
    result["status"] = (
        "ACCOUNT_SWITCHED_DURING_READ" if not account_stable else
        "DEMO_FEED_RECENT" if valid_history and current_history and result["quote_recent"] else
        "DEMO_HISTORY_ONLY_OR_STALE" if valid_history else
        "DEMO_HISTORY_INCOMPLETE"
    )
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--symbol", default="XAUUSD",
                        help="Exact broker symbol, including any suffix")
    parser.add_argument("--require-fresh", action="store_true",
                        help="Return success only when quote and all seven timeframes are recent")
    args = parser.parse_args(argv)
    if not args.symbol or len(args.symbol) > 80 or any(c.isspace() for c in args.symbol):
        parser.error("Supply the broker's exact symbol without spaces")
    try:
        import MetaTrader5 as mt5
    except ImportError:
        print(json.dumps({"status": "INSTALL_METATRADER5_PYTHON_PACKAGE"}))
        return 2
    if not mt5.initialize():
        print(json.dumps({"status": "MT5_INITIALIZE_FAILED"}))
        return 2
    try:
        report = inspect(mt5, args.symbol, round(datetime.now(timezone.utc).timestamp() * 1000))
        print(json.dumps(report, ensure_ascii=False, indent=2))
    finally:
        mt5.shutdown()
    if args.require_fresh:
        return 0 if report["status"] == "DEMO_FEED_RECENT" else 2
    return 0 if report["status"] in ("DEMO_FEED_RECENT", "DEMO_HISTORY_ONLY_OR_STALE") else 2


if __name__ == "__main__":
    sys.exit(main())
