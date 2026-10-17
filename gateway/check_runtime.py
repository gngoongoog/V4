"""Packaged runtime check and read-only Demo execution acceptance collector.

Default invocation stays import-only and never connects to MT5.  The optional
``--demo-acceptance`` mode talks only to the already-running local BrainTrade
Core API.  It never starts/stops execution and never submits, modifies or closes
an order.  It collects broker results produced by the normal journaled gateway,
persists de-duplicated local samples, and summarizes target-machine latency.
"""
from __future__ import annotations

import argparse
import csv
import ctypes
from ctypes import wintypes
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import sys
import tempfile
import time
import urllib.error
import urllib.request

CORE_URL = "http://127.0.0.1:18741"
FAST_QUEUE_TARGET_MS = 20.0
STATE_SCHEMA = 1
TIMING_FIELDS = (
    "core_pulse_ms",
    "queue_to_gateway_ms",
    "authorize_ms",
    "journal_ms",
    "other_pre_send_ms",
    "pre_send_ms",
    "queue_to_send_ms",
    "send_ms",
    "total_ms",
    "spread_points",
)
CSV_FIELDS = (
    "id",
    "created_ms",
    "brain",
    "kind",
    "signal_key",
    "command_state",
    "result_state",
    "retcode",
    "fast_path",
    "source",
    "order_check_skipped",
    "prefetched_work",
    "sent",
    "core_pulse_ms",
    "queue_to_gateway_ms",
    "authorize_ms",
    "journal_ms",
    "other_pre_send_ms",
    "pre_send_ms",
    "queue_to_send_ms",
    "send_ms",
    "total_ms",
    "spread_points",
    "queue_to_send_target_met",
    "reason",
)


def runtime_import_check(output: str | None = None) -> int:
    """Original package smoke check.  Does not initialize MT5."""
    report = {
        "python": platform.python_version(),
        "architecture": platform.machine(),
        "isolated": bool(sys.flags.isolated),
        "ok": False,
    }
    try:
        import numpy
        import MetaTrader5
        report.update(numpy=numpy.__version__, mt5=MetaTrader5.__version__, ok=True)
    except Exception as error:
        report["error"] = type(error).__name__ + ": " + str(error)
    text = json.dumps(report, ensure_ascii=False, indent=2)
    if output:
        Path(output).write_text(text, encoding="utf-8")
    print(text)
    return 0 if report["ok"] else 1


def _finite(value):
    if isinstance(value, bool) or value is None:
        return None
    try:
        number = float(value)
    except (TypeError, ValueError, OverflowError):
        return None
    return number if math.isfinite(number) else None


def percentile(values, q: float):
    """Deterministic linear percentile; returns None for an empty sample."""
    rows = sorted(v for value in values if (v := _finite(value)) is not None)
    if not rows:
        return None
    if len(rows) == 1:
        return rows[0]
    position = (len(rows) - 1) * q
    low = int(math.floor(position))
    high = int(math.ceil(position))
    if low == high:
        return rows[low]
    fraction = position - low
    return rows[low] * (1.0 - fraction) + rows[high] * fraction


def metric_stats(values):
    rows = [v for value in values if (v := _finite(value)) is not None]
    if not rows:
        return {"count": 0, "mean": None, "min": None, "max": None,
                "p50": None, "p95": None, "p99": None}
    return {
        "count": len(rows),
        "mean": sum(rows) / len(rows),
        "min": min(rows),
        "max": max(rows),
        "p50": percentile(rows, 0.50),
        "p95": percentile(rows, 0.95),
        "p99": percentile(rows, 0.99),
    }


def _count(rows, key):
    out = {}
    for row in rows:
        value = str(row.get(key, ""))
        out[value] = out.get(value, 0) + 1
    return dict(sorted(out.items()))


def _fingerprint(server: str, login: int, symbol: str, currency: str) -> str:
    raw = f"{server}\0{login}\0{symbol}\0{currency}".encode("utf-8")
    return hashlib.sha256(raw).hexdigest()[:16]


def _context(execution: dict):
    settings = execution.get("settings") or {}
    if settings.get("target") != "demo":
        raise RuntimeError("Demo acceptance refuses a Live execution target")
    if not execution.get("account_matches"):
        raise RuntimeError("Execution is not bound to the currently connected Demo account")
    try:
        server, login, symbol, currency = json.loads(execution.get("context") or "")
        login = int(login)
    except (TypeError, ValueError, json.JSONDecodeError) as error:
        raise RuntimeError("No valid bound execution context is available") from error
    if login <= 0 or not all(isinstance(value, str) and value for value in (server, symbol, currency)):
        raise RuntimeError("Bound execution identity is incomplete")
    if (int(settings.get("login") or 0), settings.get("server"), settings.get("currency")) != (login, server, currency):
        raise RuntimeError("Execution settings and current account context disagree")
    return {
        "server": server,
        "login": login,
        "symbol": symbol,
        "currency": currency,
        "account_fingerprint": _fingerprint(server, login, symbol, currency),
    }


def normalize_command(item: dict, identity: dict, source: str, existing: dict | None = None):
    request = item.get("request") or {}
    result = item.get("result") or {}
    intent = str(request.get("id") or "")
    if not intent or not isinstance(result, dict) or not result:
        return existing
    if request.get("target") != "demo":
        return existing
    if (int(request.get("login") or 0), request.get("server"), request.get("currency"), request.get("symbol")) != (
        identity["login"], identity["server"], identity["currency"], identity["symbol"]
    ):
        return existing
    if result.get("id") not in (None, intent):
        return existing
    signal = request.get("signal") or {}
    fast = result.get("fast_path") is True
    sent = _finite(result.get("send_ms")) is not None and _finite(result.get("queue_to_send_ms")) is not None
    authorize = _finite(result.get("authorize_ms")) or 0.0
    journal = _finite(result.get("journal_ms")) or 0.0
    check = _finite(result.get("check_ms")) or 0.0
    pre_send = _finite(result.get("pre_send_ms"))
    other_pre = None if pre_send is None else max(0.0, pre_send - authorize - journal - check)
    pricing = result.get("pricing") or {}
    preserved_source = (existing or {}).get("source")
    if preserved_source in ("native", "poll"):
        source = preserved_source
    elif source not in ("native", "poll"):
        source = "unlabelled"
    row = {
        "id": intent,
        "created_ms": int(request.get("created_ms") or 0),
        "brain": str(request.get("brain") or ""),
        "kind": str(request.get("kind") or ""),
        "signal_key": str(signal.get("key") or ""),
        "command_state": str(item.get("state") or ""),
        "result_state": str(result.get("state") or item.get("state") or ""),
        "retcode": result.get("retcode"),
        "fast_path": fast,
        "source": source,
        "order_check_skipped": result.get("order_check_skipped"),
        "prefetched_work": result.get("prefetched_work") is True,
        "sent": sent,
        "core_pulse_ms": None if _finite(result.get("core_pulse_us")) is None else _finite(result.get("core_pulse_us")) / 1000.0,
        "queue_to_gateway_ms": _finite(result.get("queue_to_gateway_ms")),
        "authorize_ms": _finite(result.get("authorize_ms")),
        "journal_ms": _finite(result.get("journal_ms")),
        "other_pre_send_ms": other_pre,
        "pre_send_ms": pre_send,
        "queue_to_send_ms": _finite(result.get("queue_to_send_ms")),
        "send_ms": _finite(result.get("send_ms")),
        "total_ms": _finite(result.get("total_ms")),
        "spread_points": _finite(pricing.get("spread_points")),
        "queue_to_send_target_met": result.get("queue_to_send_target_met"),
        "reason": str(result.get("reason") or ""),
    }
    return row


def merge_execution(samples: dict, execution: dict, identity: dict, source: str,
                    session_start_ms: int = 0):
    for item in execution.get("commands") or []:
        request = item.get("request") or {}
        intent = str(request.get("id") or "")
        if not intent:
            continue
        existing = samples.get(intent)
        created = int(request.get("created_ms") or 0)
        if existing is None and source in ("native", "poll") and created < session_start_ms:
            # Operator labels only commands created while this controlled capture
            # session is active; old API rows must never be mislabelled.
            continue
        row = normalize_command(item, identity, source, existing)
        if row is not None:
            samples[intent] = row
    return samples


def _timings(rows):
    return {field: metric_stats(row.get(field) for row in rows) for field in TIMING_FIELDS}


def build_report(metadata: dict, samples: dict, minimum_fast_samples: int):
    rows = sorted(samples.values(), key=lambda row: (row.get("created_ms", 0), row.get("id", "")))
    sent = [row for row in rows if row.get("sent")]
    fast = [row for row in sent if row.get("kind") == "open" and row.get("fast_path")
            and str(row.get("signal_key", "")).startswith("tick:")]
    regular = [row for row in sent if row.get("kind") == "open" and not row.get("fast_path")]
    by_source = {}
    for source in ("native", "poll", "unlabelled"):
        subset = [row for row in fast if row.get("source") == source]
        by_source[source] = {"count": len(subset), "timings": _timings(subset)}

    fast_check = None if not fast else all(row.get("order_check_skipped") is True for row in fast)
    regular_check = None if not regular else all(row.get("order_check_skipped") is False for row in regular)
    journal_present = None if not sent else all(_finite(row.get("journal_ms")) is not None for row in sent)
    prefetched = None if not fast else sum(1 for row in fast if row.get("prefetched_work")) / len(fast)
    fast_timing = _timings(fast)
    queue = fast_timing["queue_to_send_ms"]
    complete = len(fast) >= minimum_fast_samples
    p95_met = queue["p95"] is not None and queue["p95"] <= FAST_QUEUE_TARGET_MS
    p99_met = queue["p99"] is not None and queue["p99"] <= FAST_QUEUE_TARGET_MS
    contract_ok = fast_check is not False and regular_check is not False and journal_present is not False

    components = {
        field: fast_timing[field]["p95"]
        for field in ("authorize_ms", "journal_ms", "other_pre_send_ms")
        if fast_timing[field]["p95"] is not None
    }
    hotspot = max(components, key=components.get) if components else None
    if not contract_ok:
        result = "CONTRACT_FAIL"
    elif not complete:
        result = "INCOMPLETE"
    elif p95_met and p99_met:
        result = "TARGET_MET"
    else:
        result = "CHARACTERIZED_SLOW"

    warnings = []
    if len(fast) < minimum_fast_samples:
        warnings.append(f"Need {minimum_fast_samples-len(fast)} more sent Sniper samples for the configured characterization threshold")
    if not any(row.get("source") == "native" for row in fast):
        warnings.append("No operator-labelled native OnTick capture samples yet")
    if not any(row.get("source") == "poll" for row in fast):
        warnings.append("No operator-labelled 5 ms fallback polling capture samples yet")
    if any(row.get("source") == "unlabelled" for row in fast):
        warnings.append("Some historical fast samples are unlabelled; do not use them for native-vs-poll comparison")
    if any(row.get("result_state") == "unknown" for row in rows):
        warnings.append("At least one uncertain broker result exists; reconciliation must complete before judging fill behavior")
    if complete and not (p95_met and p99_met):
        warnings.append("Local queue-to-send P95/P99 exceeds the 20 ms engineering target; inspect component timings before changing any safety boundary")

    report = {
        "schema": STATE_SCHEMA,
        "generated_utc": datetime.now(timezone.utc).isoformat(),
        "result": result,
        "metadata": metadata,
        "sample_counts": {
            "recorded_results": len(rows),
            "sent_orders": len(sent),
            "sent_sniper_opens": len(fast),
            "sent_regular_opens": len(regular),
            "minimum_fast_samples": minimum_fast_samples,
        },
        "broker_result_states": _count(rows, "result_state"),
        "broker_retcodes": _count([row for row in rows if row.get("retcode") is not None], "retcode"),
        "fast_path": {
            "timings": fast_timing,
            "sources": by_source,
            "prefetched_work_fraction": prefetched,
            "queue_to_send_target_ms": FAST_QUEUE_TARGET_MS,
            "p95_target_met": p95_met if fast else None,
            "p99_target_met": p99_met if fast else None,
            "largest_local_pre_send_component_p95": hotspot,
        },
        "regular_open": {"timings": _timings(regular)},
        "contracts": {
            "demo_context_only": True,
            "fast_open_skips_order_check": fast_check,
            "regular_open_keeps_order_check": regular_check,
            "durable_journal_timing_present_before_recorded_send": journal_present,
            "collector_is_read_only": True,
        },
        "characterization_complete": complete,
        "warnings": warnings,
        "interpretation": [
            "queue_to_send_ms is BrainTrade-local latency from durable command creation to immediately before MT5 order_send; it is not broker fill latency.",
            "send_ms measures the local MT5 order_send call through broker acknowledgement and remains network/broker dependent.",
            "native and poll source labels are operator-controlled capture sessions. Label native only with the OnTick bridge attached and poll only after detaching/stopping it.",
            "This collector performs GET requests only. It cannot enable START, bind an account, or submit an order.",
        ],
    }
    return report, rows


def write_outputs(output_dir: Path, report: dict, rows: list[dict]):
    output_dir.mkdir(parents=True, exist_ok=True)
    report_path = output_dir / "demo-acceptance-latest.json"
    csv_path = output_dir / "demo-acceptance-latest.csv"
    temporary = report_path.with_suffix(".json.tmp")
    temporary.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    temporary.replace(report_path)
    with csv_path.open("w", newline="", encoding="utf-8-sig") as stream:
        writer = csv.DictWriter(stream, fieldnames=CSV_FIELDS, extrasaction="ignore")
        writer.writeheader()
        writer.writerows(rows)
    return report_path, csv_path


def save_state(path: Path, state: dict):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(state, ensure_ascii=False, indent=2), encoding="utf-8")
    temporary.replace(path)


def load_state(path: Path):
    if not path.is_file():
        return None
    data = json.loads(path.read_text(encoding="utf-8"))
    if data.get("schema") != STATE_SCHEMA or not isinstance(data.get("samples"), dict):
        raise RuntimeError("Acceptance state schema is incompatible; use --reset")
    return data


def _unseal_windows(path: Path) -> str:
    class Blob(ctypes.Structure):
        _fields_ = [("size", wintypes.DWORD), ("data", ctypes.POINTER(ctypes.c_ubyte))]
    raw = path.read_bytes()
    memory = (ctypes.c_ubyte * len(raw)).from_buffer_copy(raw)
    source, output = Blob(len(raw), memory), Blob()
    crypt = ctypes.WinDLL("crypt32", use_last_error=True)
    crypt.CryptUnprotectData.argtypes = [ctypes.POINTER(Blob), ctypes.c_void_p, ctypes.c_void_p,
                                        ctypes.c_void_p, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(Blob)]
    crypt.CryptUnprotectData.restype = wintypes.BOOL
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.LocalFree.argtypes = [ctypes.c_void_p]
    kernel.LocalFree.restype = ctypes.c_void_p
    if not crypt.CryptUnprotectData(ctypes.byref(source), None, None, None, None, 0, ctypes.byref(output)):
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        return ctypes.string_at(output.data, output.size).decode("utf-8")
    finally:
        kernel.LocalFree(output.data)


def read_ipc_key(path: Path) -> str:
    if os.name == "nt":
        return _unseal_windows(path)
    return path.read_text(encoding="utf-8")


def discover_data_dir(explicit: Path | None = None) -> Path:
    if explicit is not None:
        path = explicit.expanduser().resolve()
        if not (path / "ipc.key").is_file():
            raise RuntimeError(f"ipc.key not found under {path}")
        return path
    env = os.environ.get("BRAINTRADE_DATA_DIR")
    if env and (Path(env) / "ipc.key").is_file():
        return Path(env).resolve()
    roots = []
    for name in ("LOCALAPPDATA", "APPDATA"):
        value = os.environ.get(name)
        if value:
            base = Path(value)
            roots.extend((base / "BrainTrade", base / "BrainTrade" / "BrainTrade"))
    candidates = []
    for root in roots:
        if not root.exists():
            continue
        if (root / "ipc.key").is_file():
            candidates.append(root)
        try:
            candidates.extend(path.parent for path in root.rglob("ipc.key"))
        except OSError:
            pass
    unique = {str(path.resolve()): path.resolve() for path in candidates}
    if not unique:
        raise RuntimeError("BrainTrade data directory was not found; pass --data-dir explicitly")
    return max(unique.values(), key=lambda path: (path / "ipc.key").stat().st_mtime)


class CoreApi:
    def __init__(self, key: str, base_url: str = CORE_URL):
        self.key = key
        self.base_url = base_url.rstrip("/")
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def get(self, path: str):
        request = urllib.request.Request(
            self.base_url + path,
            headers={"Authorization": "Bearer " + self.key, "Accept": "application/json"},
            method="GET",
        )
        try:
            with self.opener.open(request, timeout=5) as response:
                return json.load(response)
        except urllib.error.URLError as error:
            raise RuntimeError("BrainTrade Core is not reachable; open BrainTrade.exe first") from error


def acceptance_cli(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Read-only BrainTrade MT5 Demo acceptance collector")
    parser.add_argument("--data-dir", type=Path)
    parser.add_argument("--output-dir", type=Path, default=Path("acceptance-output"))
    parser.add_argument("--watch-seconds", type=int, default=0,
                        help="Poll the running Core for this many seconds; 0 takes one snapshot")
    parser.add_argument("--poll-seconds", type=float, default=1.0)
    parser.add_argument("--minimum-fast-samples", type=int, default=200)
    parser.add_argument("--source", choices=("native", "poll", "auto"), default="auto",
                        help="Operator label for NEW commands in this capture session")
    parser.add_argument("--reset", action="store_true", help="Discard the persisted local acceptance sample state")
    parser.add_argument("--base-url", default=CORE_URL)
    args = parser.parse_args(argv)
    if args.watch_seconds < 0 or not 0.1 <= args.poll_seconds <= 60 or args.minimum_fast_samples < 1:
        raise RuntimeError("Invalid watch/poll/sample settings")

    data_dir = discover_data_dir(args.data_dir)
    api = CoreApi(read_ipc_key(data_dir / "ipc.key"), args.base_url)
    status = api.get("/v1/status")
    execution = api.get("/v1/execution")
    config = api.get("/v1/config").get("config") or {}
    identity = _context(execution)
    mode = str((config.get("trading") or {}).get("mode") or "")
    metadata = {
        "version": status.get("version"),
        "account_fingerprint": identity["account_fingerprint"],
        "server": identity["server"],
        "symbol": identity["symbol"],
        "currency": identity["currency"],
        "target": "demo",
        "trading_mode_at_start": mode,
        "requested_run_at_start": bool(status.get("requested_run")),
        "execution_enabled_at_start": bool(status.get("execution_enabled")),
        "capture_source": args.source,
        "source_label_contract": "operator-labelled controlled session; native=EA attached, poll=EA detached/stopped",
    }
    output_dir = args.output_dir.resolve()
    state_path = output_dir / "demo-acceptance-state.json"
    if args.reset and state_path.exists():
        state_path.unlink()
    state = load_state(state_path)
    if state is None:
        state = {"schema": STATE_SCHEMA, "version": status.get("version"),
                 "account_fingerprint": identity["account_fingerprint"], "samples": {}}
    elif (state.get("version"), state.get("account_fingerprint")) != (
            status.get("version"), identity["account_fingerprint"]):
        raise RuntimeError("Persisted acceptance state belongs to another app version/account; use --reset")
    samples = state["samples"]
    started_ms = int(time.time() * 1000)
    deadline = time.monotonic() + args.watch_seconds
    label = args.source if args.source in ("native", "poll") else "auto"
    print(f"Demo acceptance: {identity['server']} / {identity['symbol']} / {identity['currency']} / {identity['account_fingerprint']}")
    print("READ ONLY: this collector never sends orders or changes START/STOP/settings.")
    if label == "native":
        print("Source label NATIVE: keep BrainTradeSniperBridge attached and CONNECTED for this capture.")
    elif label == "poll":
        print("Source label POLL: detach/stop BrainTradeSniperBridge before this capture.")

    try:
        while True:
            status = api.get("/v1/status")
            execution = api.get("/v1/execution")
            current = _context(execution)
            if current["account_fingerprint"] != identity["account_fingerprint"]:
                raise RuntimeError("Account/context changed during capture; stopped without mixing samples")
            merge_execution(samples, execution, identity, label, started_ms)
            state["samples"] = samples
            save_state(state_path, state)
            report, rows = build_report(metadata, samples, args.minimum_fast_samples)
            write_outputs(output_dir, report, rows)
            fast_count = report["sample_counts"]["sent_sniper_opens"]
            print(f"samples={len(rows)} sent_sniper={fast_count}/{args.minimum_fast_samples} result={report['result']}", end="\r", flush=True)
            if args.watch_seconds == 0 or fast_count >= args.minimum_fast_samples or time.monotonic() >= deadline:
                break
            time.sleep(args.poll_seconds)
    except KeyboardInterrupt:
        print("\nCapture stopped by user; saved samples are retained.")
    report, rows = build_report(metadata, samples, args.minimum_fast_samples)
    report_path, csv_path = write_outputs(output_dir, report, rows)
    print("\n" + json.dumps({
        "result": report["result"],
        "sent_sniper_opens": report["sample_counts"]["sent_sniper_opens"],
        "queue_to_send_p95_ms": report["fast_path"]["timings"]["queue_to_send_ms"]["p95"],
        "queue_to_send_p99_ms": report["fast_path"]["timings"]["queue_to_send_ms"]["p99"],
        "largest_local_component_p95": report["fast_path"]["largest_local_pre_send_component_p95"],
        "json": str(report_path),
        "csv": str(csv_path),
    }, ensure_ascii=False, indent=2))
    return 1 if report["result"] == "CONTRACT_FAIL" else 0


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--demo-acceptance":
        return acceptance_cli(sys.argv[2:])
    output = sys.argv[1] if len(sys.argv) > 1 else None
    return runtime_import_check(output)


if __name__ == "__main__":
    raise SystemExit(main())
