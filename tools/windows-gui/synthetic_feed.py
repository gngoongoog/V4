"""Publish an explicitly synthetic Demo snapshot to the isolated GUI-test Core.

This is a test fixture, not an MT5 connection. It never starts execution,
connects to a broker, or submits orders. The caller must launch the actual
packaged app with its gateway disabled in a disposable hosted Windows runner.
"""
from __future__ import annotations

import json
import os
import sys
import time
import urllib.request
from pathlib import Path

if (os.name != "nt" or os.environ.get("GITHUB_ACTIONS") != "true"
        or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"
        or os.environ.get("BRAINTRADE_GATEWAY_DISABLED") != "1"):
    raise SystemExit("Synthetic feed is restricted to the isolated Windows GUI CI runner")

root = Path(__file__).resolve().parents[2]
home = Path(sys.argv[1]).resolve()
if not home.name.startswith("braintrade-rc2-gui-"):
    raise SystemExit("Refusing a data directory outside this GUI test")
sys.path[:0] = [str(root / "scripts"), str(root / "gateway")]
from check_windows_portable import unseal
from mt5_gateway import empty_snapshot

key = unseal(home / "ipc.key")
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
snapshot = empty_snapshot("")
snapshot.update(connected=True, login=170000001, server="Synthetic-CI-Demo",
                currency="USD", symbol="XAUUSD", demo=True,
                account_trade_mode=0, margin_mode=2,
                trade_allowed=False, trade_expert=False,
                balance=10000.0, equity=10000.0, margin=0.0, margin_free=10000.0,
                bid=2500.0, ask=2500.1, point=0.01, tick_size=0.01,
                volume_min=0.01, volume_step=0.01, volume_max=100.0,
                tick_value_loss=1.0, tick_value_profit=1.0)

while True:
    now = time.time_ns() // 1_000_000
    snapshot.update(quote_time_ms=now, source_quote_time_ms=now, tick_age_ms=0)
    request = urllib.request.Request(
        "http://127.0.0.1:18741/v1/gateway/snapshot",
        data=json.dumps(snapshot).encode("utf-8"),
        headers={"Authorization": "Bearer " + key, "Content-Type": "application/json"},
    )
    with opener.open(request, timeout=8) as response:
        if json.load(response).get("ok") is not True:
            raise RuntimeError("Core rejected the synthetic GUI-test snapshot")
    time.sleep(0.25)
