"""Explicit, read-only exporter for MT5 **Demo** deal-history research.

No broker sends, position changes, credentials or background polling. This
script can produce ONLY a caller-supplied evidence payload for 3F-L research,
NOT a broker attestation, proof of complete history, or a trade approval.

Import this module for unit tests without importing the MetaTrader5 package.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import math
from pathlib import Path
import re
import sys
import time

IDENT = re.compile(r"^[A-Za-z0-9_.:-]{1,100}$")
MAX_DEALS = 5000
MAX_WINDOW_MS = 31 * 24 * 60 * 60 * 1000
VALID_IN = 0
VALID_OUT = 1
VALID_INOUT = 2


class DemoEvidenceError(ValueError):
    pass


def positive(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value) and 0 < value <= 1e12


def finite_cash(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value) and abs(value) <= 1e12


def identity(value, name):
    if isinstance(value, bool) or not isinstance(value, (str, int)) or (type(value) is int and value <= 0):
        raise DemoEvidenceError(f"{name} must be a valid bounded text or positive numeric identifier")
    value = str(value)
    if IDENT.fullmatch(value) is None:
        raise DemoEvidenceError(f"{name} must be a bounded alphanumeric identifier")
    return value


def validate_mapping(mapping):
    if not isinstance(mapping, dict) or len(mapping) > MAX_DEALS:
        raise DemoEvidenceError("Intent mapping must be a bounded JSON object")
    result = {}
    for position_id, intent_id in mapping.items():
        if not isinstance(position_id, str) or not position_id.isdecimal() or int(position_id) <= 0:
            raise DemoEvidenceError("Intent mapping keys must be positive MT5 position IDs")
        if not isinstance(intent_id, str):
            raise DemoEvidenceError("Synthetic intent ID must be a string")
        if int(position_id) in result:
            raise DemoEvidenceError("Duplicate normalized position ID in intent mapping")
        result[int(position_id)] = identity(intent_id, "Synthetic intent ID")
    return result


def capture(mt5, symbol, from_ms, to_ms, mapping=None, now_ms=None):
    """Read only MT5 terminal/account + history; refuse account switches.

    Returns a separate {evidence,export_metadata} object intended to be copied
    into the 3F-L request next to a synthetic bridge scenario. Never claims
    complete history or auto-guesses the synthetic intent-to-position mapping.
    """
    symbol = identity(symbol, "Symbol")
    if not isinstance(from_ms, int) or isinstance(from_ms, bool) or not isinstance(to_ms, int) or isinstance(to_ms, bool):
        raise DemoEvidenceError("UTC timestamps must be integer milliseconds")
    if from_ms <= 0 or to_ms <= from_ms or to_ms - from_ms > MAX_WINDOW_MS:
        raise DemoEvidenceError("Deal history window must be positive and at most 31 days")
    now_ms = int(time.time() * 1000) if now_ms is None else now_ms
    if not isinstance(now_ms, int) or now_ms <= 0 or to_ms > now_ms:
        raise DemoEvidenceError("Refusing future-dated or invalid history window")
    mapping = validate_mapping(mapping or {})
    term = mt5.terminal_info()
    account = mt5.account_info()
    if term is None or not getattr(term, "connected", False) or account is None:
        raise DemoEvidenceError("MT5 Demo terminal/account unavailable")
    demo_constant = getattr(mt5, "ACCOUNT_TRADE_MODE_DEMO", None)
    if demo_constant is None or getattr(account, "trade_mode", None) != demo_constant:
        raise DemoEvidenceError("Real/contest/unverified MT5 account refused: only Demo allowed")
    broker_login = getattr(account, "login", None)
    if type(broker_login) is not int or broker_login <= 0:
        raise DemoEvidenceError("A valid positive MT5 broker login is required")
    login = identity(broker_login, "MT5 login")
    server = identity(getattr(account, "server", ""), "MT5 server")
    currency = identity(getattr(account, "currency", ""), "Account currency")
    if not getattr(mt5, "symbol_info", lambda _: None)(symbol):
        raise DemoEvidenceError("Symbol is not available in MT5")
    before = (login, server, currency, getattr(account, "trade_mode", None))
    try:
        start = datetime.fromtimestamp(from_ms / 1000, tz=timezone.utc)
        end = datetime.fromtimestamp((to_ms - 1) / 1000, tz=timezone.utc)
        deals = mt5.history_deals_get(start, end)
    except (OverflowError, ValueError, OSError) as exc:
        raise DemoEvidenceError("MT5 deal-history query failed") from exc
    after_term = mt5.terminal_info()
    after = mt5.account_info()
    if after_term is None or not getattr(after_term, "connected", False) or after is None or (
        str(getattr(after, "login", "")),
        str(getattr(after, "server", "")),
        str(getattr(after, "currency", "")),
        getattr(after, "trade_mode", None)
    ) != before:
        raise DemoEvidenceError("MT5 account switched or disconnected during export")
    if deals is None:
        raise DemoEvidenceError("Broker history is unavailable (None != empty history)")
    if len(deals) > MAX_DEALS:
        raise DemoEvidenceError("More than 5000 deals: narrow the UTC research window")
    seen = set()
    rows = []
    excluded_symbols = 0
    for d in deals:
        if getattr(d, "symbol", None) != symbol:
            excluded_symbols += 1
            continue
        ticket = getattr(d, "ticket", 0)
        position_id = getattr(d, "position_id", 0)
        if not isinstance(ticket, int) or ticket <= 0 or not isinstance(position_id, int) or position_id <= 0:
            raise DemoEvidenceError("MT5 deal identity not supported by the audit")
        if ticket in seen:
            raise DemoEvidenceError("Duplicate broker deal ticket; export aborted")
        seen.add(ticket)
        seconds = getattr(d, "time", 0)
        event_ms = getattr(d, "time_msc", None)
        if event_ms is None or event_ms == 0:
            if not isinstance(seconds, int) or seconds <= 0:
                raise DemoEvidenceError("Deal lacks authoritative time")
            event_ms = seconds * 1000
        if not isinstance(event_ms, int) or event_ms < from_ms or event_ms >= to_ms or event_ms > now_ms:
            raise DemoEvidenceError("Deal outside requested UTC window or in future")
        entry = getattr(d, "entry", -1)
        if entry not in (VALID_IN, VALID_OUT, VALID_INOUT):
            raise DemoEvidenceError(f"Unsupported broker close-by/deal entry code: {entry}")
        direction = {VALID_IN: "in", VALID_OUT: "out", VALID_INOUT: "inout"}[entry]
        volume = getattr(d, "volume", None)
        price = getattr(d, "price", None)
        economics = [getattr(d, k, None) for k in ("profit", "commission", "swap", "fee")]
        if not positive(volume) or not positive(price) or volume > 10000 or not all(map(finite_cash, economics)):
            raise DemoEvidenceError("Invalid/unsupported MT5 broker volume, price or economics")
        rows.append({
            "ticket": ticket,
            "position_id": position_id,
            "intent_id": mapping.get(position_id),
            "direction": direction,
            "volume": float(volume),
            "price": float(price),
            "time_msc": event_ms,
            "profit": float(economics[0]),
            "commission": float(economics[1]),
            "swap": float(economics[2]),
            "fee": float(economics[3])
        })
    rows.sort(key=lambda d: (d["time_msc"], d["ticket"]))
    # Account switched after the final read would be outside this bounded call.
    # This is provenance only, not a cryptographic broker attestation.
    return {
        "evidence": {
            "source": "caller_supplied_mt5_demo_deals",
            "account": login,
            "server": server,
            "symbol": symbol,
            "currency": currency,
            "demo": True,
            "captured_ms": now_ms,
            "complete_history": False,
            "deals": rows
        },
        "export_metadata": {
            "kind": "local_demo_read_only_unattested",
            "window_from_ms": from_ms,
            "window_to_ms_exclusive": to_ms,
            "account_identity_checked_before_after_read": True,
            "mapping_origin": "operator_supplied_position_id_to_synthetic_intent_id",
            "mapped_positions": len(mapping),
            "excluded_other_symbol_deals": excluded_symbols,
            "complete_history_never_claimed": True,
            "warning": "Requires a separate Stage 3F-K synthetic bridge scenario. MT5 Demo read is not broker attestation, complete history proof, or PnL certification."
        }
    }


def main(argv=None):
    p = argparse.ArgumentParser(description="READ-ONLY, explicit MT5 Demo deal export for offline research")
    p.add_argument("--symbol", required=True)
    p.add_argument("--from-ms", required=True, type=int, help="UTC inclusive start in Unix milliseconds")
    p.add_argument("--to-ms", required=True, type=int, help="UTC exclusive end in Unix milliseconds")
    p.add_argument("--map-position-ids", help="Optional local JSON object: broker position ID -> synthetic intent ID")
    p.add_argument("--output", help="Optional output JSON path (otherwise print to stdout)")
    args = p.parse_args(argv)
    try:
        mapping = {}
        if args.map_position_ids:
            file = Path(args.map_position_ids)
            if file.stat().st_size > 100_000:
                raise DemoEvidenceError("Position mapping file too large")
            mapping = json.loads(file.read_text(encoding="utf-8"))
        try:
            import MetaTrader5 as mt5
        except ImportError as exc:
            raise DemoEvidenceError("Install the MetaTrader5 Python package in the terminal environment") from exc
        if not mt5.initialize():
            raise DemoEvidenceError("Could not connect to local MT5 terminal")
        try:
            payload = capture(mt5, args.symbol, args.from_ms, args.to_ms, mapping)
        finally:
            mt5.shutdown()
        raw = json.dumps(payload, ensure_ascii=False, indent=2, allow_nan=False)
        if args.output:
            target = Path(args.output)
            if target.exists():
                raise DemoEvidenceError("Refusing to overwrite an existing evidence file")
            with target.open("x", encoding="utf-8") as fd:
                fd.write(raw + "\n")
            print(f"Saved unverified Demo research evidence: {target}", file=sys.stderr)
        else:
            print(raw)
        return 0
    except (DemoEvidenceError, OSError, json.JSONDecodeError) as exc:
        print(f"Research evidence export refused: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
