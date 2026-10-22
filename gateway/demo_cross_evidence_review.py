"""Offline, read-only cross-check of two MT5 Demo research exports.

No MT5 connection, order submission, core state changes, or claim of authenticated
broker outcomes. Derived slippage statistics are illustrative only.
"""
from __future__ import annotations
import argparse
import json
import math
from pathlib import Path
import sys

MAX_BYTES = 8_000_000
EPS = 1e-8

class EvidenceError(ValueError):
    pass

def require(ok, reason):
    if not ok:
        raise EvidenceError(reason)

def finite(v, positive=False):
    return (not isinstance(v, bool) and isinstance(v, (int, float))
            and math.isfinite(v) and abs(v) <= 1e12
            and (not positive or v > 0))

def integer_id(v):
    return type(v) is int and v > 0

def close(a, b):
    return (finite(a) and finite(b)
            and abs(a-b) <= max(EPS, max(abs(a), abs(b))*1e-9))

def percentile(values, p):
    a = sorted(values)
    i = (len(a)-1) * p/100
    low = math.floor(i)
    high = math.ceil(i)
    return a[low] if low == high else a[low]*(high-i)+a[high]*(i-low)

def compare(deal_export, price_export):
    require(isinstance(deal_export, dict) and isinstance(price_export, dict),
            "Both exports must be JSON objects")
    d = deal_export.get("evidence")
    meta = deal_export.get("export_metadata")
    require(isinstance(d, dict) and isinstance(meta, dict), "Missing original exporter metadata")
    require(d.get("source") == "caller_supplied_mt5_demo_deals"
            and meta.get("kind") == "local_demo_read_only_unattested"
            and price_export.get("source") == "locally_observed_mt5_demo_unattested"
            and price_export.get("policy") == "read_only_demo_historical_price_audit_v1",
            "Unrecognized export provenance")
    require(d.get("demo") is True and price_export.get("demo") is True,
            "REAL / contest or unknown account is not supported")
    require(d.get("complete_history") is False
            and price_export.get("complete_history") is False,
            "Export must not assert historical completeness")
    for name in ("account", "server", "symbol", "currency"):
        v = d.get(name)
        require(isinstance(v, str) and bool(v) and v == price_export.get(name),
                "Cross-export account/symbol identity mismatch: "+name)
    start = price_export.get("window_start_utc_ms")
    end = price_export.get("window_end_utc_ms_exclusive")
    mstart = meta.get("window_from_ms")
    mend = meta.get("window_to_ms_exclusive")
    require(all(integer_id(x) for x in (start, end, mstart, mend))
            and mstart <= start < end <= mend,
            "Price-audit UTC interval must fall within deal export interval")
    tick_size = price_export.get("tick_size")
    max_age = price_export.get("requested_max_quote_age_ms")
    require(finite(tick_size, positive=True)
            and type(max_age) is int and 1 <= max_age <= 3000,
            "Missing broker tick size or invalid quote-staleness bound")
    deals = d.get("deals")
    prices = price_export.get("rows")
    require(isinstance(deals, list) and len(deals) <= 5000
            and isinstance(prices, list) and len(prices) <= 300,
            "Missing or oversized research arrays")
    expected = {}
    all_tickets = set()
    for row in deals:
        require(isinstance(row, dict), "Invalid deal record")
        ticket = row.get("ticket")
        pos = row.get("position_id")
        ts = row.get("time_msc")
        require(integer_id(ticket) and integer_id(pos) and integer_id(ts)
                and mstart <= ts < mend and ticket not in all_tickets,
                "Invalid/duplicate deal identity or UTC timestamp")
        all_tickets.add(ticket)
        require(row.get("direction") in ("in", "out", "inout")
                and finite(row.get("volume"), positive=True)
                and finite(row.get("price"), positive=True)
                and all(finite(row.get(x)) for x in ("profit", "commission", "swap", "fee")),
                "Invalid deal direction, volume, price or economics")
        if start <= ts < end:
            expected[ticket] = row
    reported = {}
    for row in prices:
        require(isinstance(row, dict), "Invalid price report record")
        ticket = row.get("deal_ticket")
        require(integer_id(ticket) and ticket not in reported,
                "Duplicate or invalid ticket in price report")
        reported[ticket] = row
    missing = sorted(set(expected)-set(reported))
    extra = sorted(set(reported)-set(expected))
    mismatch = []
    unknown_quote = []
    deviations = []
    spreads = []
    for ticket in sorted(set(expected) & set(reported)):
        drow, prow = expected[ticket], reported[ticket]
        expected_entry = {"in": 0, "out": 1, "inout": 2}[drow["direction"]]
        ok = (prow.get("position_id") == drow["position_id"]
              and prow.get("deal_time_ms") == drow["time_msc"]
              and prow.get("deal_entry_code") == expected_entry
              and close(prow.get("lots"), drow["volume"])
              and close(prow.get("execution_price"), drow["price"])
              and close(prow.get("fees_and_commission_booked_on_supplied_deal"),
                        drow["commission"]+drow["fee"])
              and close(prow.get("broker_reported_profit"), drow["profit"])
              and close(prow.get("broker_reported_swap"), drow["swap"]))
        delta = prow.get("indicative_adverse_deviation_ticks")
        reference = prow.get("reference_quote_price")
        age = prow.get("reference_quote_age_ms")
        spread = prow.get("historical_spread_ticks")
        side = prow.get("broker_deal_side")
        if delta is None:
            ok = ok and reference is None and age is None and spread is None
        else:
            ok = (ok and drow["direction"] in ("in", "out")
                  and side in ("buy", "sell") and finite(delta)
                  and finite(reference, positive=True)
                  and type(age) is int and 0 <= age <= max_age
                  and finite(spread) and spread >= 0)
            if ok:
                implied = ((drow["price"]-reference) if side == "buy" else
                           (reference-drow["price"])) / tick_size
                ok = close(implied, delta)
        if not ok:
            mismatch.append(ticket)
        elif delta is None:
            unknown_quote.append(ticket)
        else:
            deviations.append(delta)
            spreads.append(spread)
    count = price_export.get("deal_count")
    valid = price_export.get("deals_with_valid_prior_quote")
    invalid = price_export.get("deals_missing_reference")
    require(type(count) is int and type(valid) is int and type(invalid) is int
            and count == len(prices) and valid >= 0 and invalid >= 0
            and valid+invalid == count, "Contradictory price report counts")
    if sum(x.get("indicative_adverse_deviation_ticks") is not None for x in prices) != valid:
        mismatch.append("quote_count_header")
    if not missing and not extra and not mismatch:
        mean = sum(deviations)/len(deviations) if deviations else None
        reported_mean = price_export.get("mean_indicative_adverse_deviation_ticks")
        if (mean is None and reported_mean is not None) or (
                mean is not None and not close(mean, reported_mean)):
            mismatch.append("quote_mean_header")
    status = ("contradictory_exported_records" if extra or mismatch else
              "insufficient_or_sparse_reference" if missing or unknown_quote or len(deviations) < 30
              else "descriptive_sample_unverified")
    return {
        "status": status,
        "policy": "offline_demo_export_consistency_v1",
        "account": d["account"], "server": d["server"],
        "currency": d["currency"], "symbol": d["symbol"],
        "window_utc_ms": [start, end],
        "missing_price_report_tickets": missing,
        "unexpected_price_report_tickets": extra,
        "contradictory_tickets_or_headers": mismatch,
        "lacking_prior_quote_tickets": unknown_quote,
        "matched_quote_samples": len(deviations),
        "minimal_descriptive_sample": 30,
        "adverse_deviation_ticks": {
            "p50": percentile(deviations,50) if deviations else None,
            "p95": percentile(deviations,95) if deviations else None,
            "p99": percentile(deviations,99) if deviations else None,
            "maximum": max(deviations) if deviations else None,
            "adverse_fraction": sum(x>0 for x in deviations)/len(deviations)
                                if deviations else None,
        },
        "historical_spread_ticks": {
            "p50": percentile(spreads,50) if spreads else None,
            "p95": percentile(spreads,95) if spreads else None,
        },
        "history_proven_complete": False,
        "mt5_connection_performed": False,
        "broker_order_sent": False,
        "account_or_history_authenticated": False,
        "profitability_or_execution_quality_certified": False,
        "warning": "These are caller-supplied local MT5 Demo exports, not broker-attested records or confirmed executable quotes. Descriptive observations cannot authorize trading."
    }

def read_json(path):
    p = Path(path)
    require(p.stat().st_size <= MAX_BYTES, "JSON exceeds 8 MB")
    return json.loads(p.read_text(encoding="utf-8"))

def main(argv=None):
    a = argparse.ArgumentParser(description="OFFLINE Demo cross-export consistency; no MT5 calls")
    a.add_argument("--deals", required=True)
    a.add_argument("--prices", required=True)
    a.add_argument("--output")
    args = a.parse_args(argv)
    try:
        result = compare(read_json(args.deals), read_json(args.prices))
        payload = json.dumps(result, indent=2, ensure_ascii=False, allow_nan=False)
        if args.output:
            with Path(args.output).open("x", encoding="utf-8") as f:
                f.write(payload+"\n")
        else:
            print(payload)
        return 0
    except (EvidenceError, OSError, json.JSONDecodeError) as e:
        print("Offline Demo evidence comparison refused: "+str(e), file=sys.stderr)
        return 2

if __name__ == "__main__":
    raise SystemExit(main())
