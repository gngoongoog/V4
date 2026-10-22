"""Explicit single-session, read-only MT5 DEMO research evidence bundle.

Combines bounded deal export, indicative quote audit and offline consistency in
one operator-triggered run. No order_send, no Core connection, no auto trading.
Local SHA-256 hashes detect accidental artifact edits, NOT broker authenticity.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import time

import demo_evidence_export as exporter
import demo_historical_price_audit as prices
import demo_cross_evidence_review as consistency

SCHEMA = "braintrade-demo-readonly-bundle-v1"
MAX_HOURS = 24
MAX_BUNDLE_BYTES = 12_000_000

class BundleError(ValueError):
    pass

def canonical(obj):
    return json.dumps(obj, sort_keys=True, ensure_ascii=False,
                      separators=(",", ":"), allow_nan=False).encode("utf-8")

def digest(obj):
    return hashlib.sha256(canonical(obj)).hexdigest()

def build_bundle(mt5, symbol, from_ms, to_ms, *, mapping=None,
                 max_quote_age_ms=1500, now_ms=None):
    """Runs only three independently bounded research modules, same Demo session.

    If MT5's history changes between calls, cross-review records contradictions.
    It never silently selects winners between conflicting observations.
    """
    now_ms = int(time.time()*1000) if now_ms is None else now_ms
    deal_export = exporter.capture(mt5,symbol,from_ms,to_ms,
                                   mapping=mapping,now_ms=now_ms)
    price_export = prices.capture(mt5,symbol,from_ms,to_ms,
                                  max_quote_age_ms=max_quote_age_ms,
                                  now_ms=now_ms)
    review = consistency.compare(deal_export,price_export)
    account = deal_export["evidence"]["account"]
    if not (account == price_export["account"] == review["account"]):
        raise BundleError("MT5 Demo login switched between bundled reads")
    if deal_export["evidence"]["server"] != price_export["server"]:
        raise BundleError("MT5 Demo server switched between bundled reads")
    data = {"deal_export":deal_export,
            "price_export":price_export,
            "cross_evidence_review":review}
    manifest = {
        "schema":SCHEMA,
        "source":"operator_local_demo_session_unattested",
        "account":account,
        "symbol":review["symbol"],
        "window_start_ms":from_ms,
        "window_end_ms_exclusive":to_ms,
        "captured_ms":now_ms,
        "component_sha256":{name:digest(value) for name,value in data.items()},
        "checksums_not_broker_signature":True,
        "browser_or_network_upload":False,
        "mt5_order_send":False,
        "history_completeness_proven":False,
        "warning":"Hashes only detect modifications after this local export; they do not authenticate MT5, prove history completeness, execution quality or trading profitability."
    }
    result = {"manifest":manifest, **data}
    if len(canonical(result)) > MAX_BUNDLE_BYTES:
        raise BundleError("Evidence bundle exceeds 12 MB")
    return result

def verify_bundle(payload):
    """Offline re-computation, independent of MT5 and any user trust assertion."""
    if not isinstance(payload,dict) or set(payload) != {
            "manifest","deal_export","price_export","cross_evidence_review"}:
        raise BundleError("Unexpected evidence bundle schema")
    manifest=payload["manifest"]
    if not isinstance(manifest,dict) or manifest.get("schema")!=SCHEMA:
        raise BundleError("Unrecognized evidence bundle version")
    if (manifest.get("source")!="operator_local_demo_session_unattested"
            or manifest.get("checksums_not_broker_signature") is not True
            or manifest.get("mt5_order_send") is not False
            or manifest.get("history_completeness_proven") is not False):
        raise BundleError("Contradictory evidence-source manifest")
    hashes=manifest.get("component_sha256")
    data={k:payload[k] for k in ("deal_export","price_export","cross_evidence_review")}
    if not isinstance(hashes,dict) or set(hashes)!=set(data):
        raise BundleError("Evidence component hashes missing")
    for name,value in data.items():
        if hashes[name] != digest(value):
            raise BundleError("Checksum mismatch in component "+name)
    d=data["deal_export"]["evidence"]
    p=data["price_export"]
    if (manifest.get("account")!=d.get("account")
            or manifest.get("symbol")!=d.get("symbol")
            or manifest.get("window_start_ms")!=p.get("window_start_utc_ms")
            or manifest.get("window_end_ms_exclusive")!=p.get("window_end_utc_ms_exclusive")
            or manifest.get("captured_ms")!=d.get("captured_ms")
            or manifest.get("captured_ms")!=p.get("captured_utc_ms")):
        raise BundleError("Manifest/account/time mismatch")
    rechecked=consistency.compare(data["deal_export"],data["price_export"])
    if digest(rechecked)!=digest(data["cross_evidence_review"]):
        raise BundleError("Stored review differs from offline recalculation")
    return {"schema":SCHEMA,"checksums_match":True,
            "offline_recalculation_matches":True,
            "review_status":rechecked["status"],
            "broker_authenticated":False,
            "history_completeness_proven":False,
            "broker_order_sent":False,
            "warning":"Self-consistent local JSON only. A checksum does not prove data were obtained from MT5 or the broker."}

def load_file(path):
    target=Path(path)
    if target.stat().st_size>MAX_BUNDLE_BYTES:
        raise BundleError("Bundle exceeds maximum size")
    return json.loads(target.read_text(encoding="utf-8"))

def write_new(path,data):
    """Never overwrite existing output; leave no partial final file on errors."""
    target=Path(path)
    output=json.dumps(data,indent=2,ensure_ascii=False,allow_nan=False)+"\n"
    encoded=output.encode("utf-8")
    if len(encoded)>MAX_BUNDLE_BYTES:
        raise BundleError("Bundle exceeds size limit")
    if target.exists():
        raise BundleError("Refusing to overwrite existing evidence")
    # os.link atomically creates the final path iff it does not already exist.
    temp_path=None
    try:
        with tempfile.NamedTemporaryFile("wb",prefix=".braintrade-evidence-",
                                         suffix=".tmp",dir=target.parent,
                                         delete=False) as handle:
            temp_path=Path(handle.name)
            handle.write(encoded)
            handle.flush()
            os.fsync(handle.fileno())
        os.link(temp_path,target)
    finally:
        if temp_path is not None:
            temp_path.unlink(missing_ok=True)

def main(argv=None):
    parser=argparse.ArgumentParser(description="One-shot DEMO-only research evidence bundle; no trading")
    parser.add_argument("--symbol",help="Broker symbol e.g. XAUUSD for capture")
    parser.add_argument("--hours",type=int,default=2,
                        help="UTC lookback up to 24h (only with --symbol)")
    parser.add_argument("--max-quote-age-ms",type=int,default=1500)
    parser.add_argument("--map-position-ids",help="Optional reviewed JSON position ID -> intent ID")
    parser.add_argument("--verify",help="Offline verify a previously saved bundle (no MT5)")
    parser.add_argument("--output",help="Required for capture; optional for offline verification")
    args=parser.parse_args(argv)
    try:
        if args.verify:
            if args.symbol or args.map_position_ids:
                raise BundleError("Offline --verify must not request MT5 capture")
            result=verify_bundle(load_file(args.verify))
        else:
            if not args.symbol or not args.output or not 1<=args.hours<=MAX_HOURS:
                raise BundleError("Capture requires --symbol, --output and 1–24 hours")
            mapping={}
            if args.map_position_ids:
                mapping=load_file(args.map_position_ids)
            try:
                import MetaTrader5 as mt5
            except ImportError as e:
                raise BundleError("MetaTrader5 Python package is not installed") from e
            if not mt5.initialize():
                raise BundleError("Local MT5 Demo terminal initialization failed")
            try:
                now_ms=int(time.time()*1000)
                to_ms=now_ms-1000
                from_ms=to_ms-args.hours*3600000
                result=build_bundle(mt5,args.symbol,from_ms,to_ms,mapping=mapping,
                    max_quote_age_ms=args.max_quote_age_ms,now_ms=now_ms)
            finally:
                mt5.shutdown()
        if args.output:
            write_new(args.output,result)
            print("Saved local unverified Demo research bundle: "+args.output,file=sys.stderr)
        else:
            print(json.dumps(result,indent=2,ensure_ascii=False,allow_nan=False))
        return 0
    except (BundleError,exporter.DemoEvidenceError,consistency.EvidenceError,
            OSError,ValueError,KeyError,TypeError) as e:
        print("DEMO research export/verification refused: "+str(e),file=sys.stderr)
        return 2

if __name__=="__main__":
    raise SystemExit(main())
