"""Read-only MT5 Demo preflight from the existing authenticated Core status.

No MetaTrader initialize(), order_send(), trading mutations or Broker acceptance
claims. A current account snapshot and quote do NOT prove a trading lifecycle.
Outputs intentionally contain no login, server, IPC token, MT5 path or PnL.
"""
from __future__ import annotations

import html
import json
import math
import re
import time
from pathlib import Path


def _positive_number(value):
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return None
    value = float(value)
    return value if math.isfinite(value) and value > 0 else None


def evaluate(status: dict, config: dict, now_ms: int | None = None) -> dict:
    now_ms = int(time.time() * 1000) if now_ms is None else int(now_ms)
    telemetry = status.get("telemetry") if isinstance(status, dict) else None
    t = telemetry if isinstance(telemetry, dict) else {}
    configured_symbol = (config.get("symbol") if isinstance(config, dict) else None)
    symbol = str(t.get("symbol") or "")[:80]
    configured = str(configured_symbol or "")[:80]
    connected = status.get("gateway_fresh") is True and t.get("connected") is True
    demo = (connected and t.get("demo") is True
            and type(t.get("account_trade_mode")) is int
            and t["account_trade_mode"] == 0)
    expected_symbol_matches = bool(symbol and configured and symbol == configured)
    bid, ask = _positive_number(t.get("bid")), _positive_number(t.get("ask"))
    price_ok = bid is not None and ask is not None and ask >= bid
    raw_age = t.get("tick_age_ms")
    observed_ms = status.get("last_received_ms")
    quote_age = None
    if (price_ok and type(raw_age) in (float, int) and math.isfinite(raw_age)
            and raw_age >= 0 and type(observed_ms) is int
            and 0 < observed_ms <= now_ms + 2000):
        quote_age = float(raw_age) + max(0, now_ms - observed_ms)
    source_ms = t.get("source_quote_time_ms")
    if source_ms is None:
        source_ms = t.get("quote_time_ms")
    clock_ok = (type(source_ms) is int and source_ms > 0
                and source_ms <= now_ms + 2000
                and source_ms <= int(t.get("quote_time_ms") or source_ms))
    fresh = bool(connected and price_ok and clock_ok and quote_age is not None
                 and quote_age <= 3000 and now_ms - source_ms <= 3000)
    symbol_error = str(t.get("error") or "")
    exact_missing = (symbol != "" and symbol_error.startswith(
        f"Symbol {symbol} is unavailable; choose its exact broker name in Settings."))
    if not connected:
        reason = "BROKER_DISCONNECTED"
    elif not demo:
        reason = "DEMO_ACCOUNT_NOT_VERIFIED"
    elif not expected_symbol_matches:
        reason = "CONFIGURED_SYMBOL_MISMATCH"
    elif exact_missing:
        reason = "SELECTED_SYMBOL_UNAVAILABLE"
    elif not price_ok:
        reason = "NO_VALID_BID_ASK"
    elif not clock_ok or quote_age is None:
        reason = "QUOTE_TIME_UNVERIFIED"
    elif not fresh:
        reason = "QUOTE_STALE_OR_SESSION_INACTIVE"
    else:
        reason = "CURRENT_DEMO_ACCOUNT_AND_QUOTE_OBSERVED"
    # Never export the account login or other broker identity as plain text.
    # The original status may contain credentials/identity; only allow-listed
    # neutral fields are serialized below, not the source status/config.
    return {
        "schema": "v25-g2-demo-readiness-readonly-v1",
        "result": reason,
        "account_snapshot_status": "VERIFIED_DEMO" if demo else "NOT_VERIFIED",
        "requested_run": bool(status.get("requested_run")),
        "execution_enabled": bool(status.get("execution_enabled")),
        "broker_connected": connected,
        "configured_symbol": configured,
        "telemetry_symbol": symbol,
        "selected_symbol_exact_match": expected_symbol_matches,
        "symbol_unavailable_at_mt5": bool(exact_missing),
        "quote_status": "FRESH" if fresh else "NOT_TRUSTED",
        "quote_age_ms": round(quote_age) if quote_age is not None else None,
        "observed_quote_clock_valid": clock_ok,
        "used_margin_reported": (_positive_number(t.get("margin")) is not None
                                 or t.get("margin") == 0),
        "no_orders_sent_by_this_tool": True,
        "broker_lifecycle_acceptance": "NOT_TESTED",
        "ea_metaeditor_acceptance": "NOT_TESTED",
        "windows_gui_acceptance": "NOT_TESTED",
        "note": "Account/market telemetry is provided by running Core+Gateway, not independently verified against broker Deals. No orders were sent. No real-money trading authorization.",
    }


def save_report(report: dict, output: Path) -> tuple[Path, Path]:
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    raw = output / "demo-readiness-latest.json"
    page = output / "demo-readiness-latest.html"
    raw.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n",
                   encoding="utf-8")
    lines = "".join(
        "<tr><th>" + html.escape(str(key)) + "</th><td>" +
        html.escape(str(value)) + "</td></tr>"
        for key, value in report.items()
    )
    page.write_text(
        '<!doctype html><html lang="ar" dir="rtl"><head><meta charset="UTF-8">'
        '<title>BRAINTRADE V25 — Demo Readiness (Read Only)</title></head><body>'
        '<h2>فحص MT5 Demo للقراءة فقط — ليس اعتماد تداول</h2>'
        '<p>لا يُرسل الفحص أي أوامر أو ينشّط التداول، ويخفي هوية الحساب.</p>'
        '<table border="1">' + lines + '</table></body></html>',
        encoding="utf-8",
    )
    return raw, page
