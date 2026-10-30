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

synthetic_stop=None
while True:
    now = time.time_ns() // 1_000_000
    allow_manual=(home / 'synthetic-manual-allowed.flag').exists()
    snapshot.update(trade_allowed=allow_manual,trade_expert=allow_manual)
    # An explicit CI-only marker exposes a fabricated owned position for the
    # native row-close test. The real MT5 Gateway remains disabled throughout.
    snapshot['positions'] = ([dict(ticket=777001, identifier=777001, magic=410600,
        symbol='XAUUSD', side='BUY', volume=0.1, price_open=2499.0,
        price_current=2500.0, profit=10.0, sl=synthetic_stop if synthetic_stop is not None else 2490.0, tp=2510.0)]
        if (home / 'synthetic-owned-position.flag').exists() else [])
    snapshot.update(quote_time_ms=now, source_quote_time_ms=now, tick_age_ms=0)
    request = urllib.request.Request(
        "http://127.0.0.1:18741/v1/gateway/snapshot",
        data=json.dumps(snapshot).encode("utf-8"),
        headers={"Authorization": "Bearer " + key, "Content-Type": "application/json"},
    )
    with opener.open(request, timeout=8) as response:
        if json.load(response).get("ok") is not True:
            raise RuntimeError("Core rejected the synthetic GUI-test snapshot")
    if (home / 'synthetic-protect-observe.flag').exists() and synthetic_stop is None:
        # Verify a single Core modification intent and acknowledge it without
        # importing MetaTrader5. The next fake snapshot exposes the new stop.
        poll=urllib.request.Request('http://127.0.0.1:18741/v1/execution/gateway',headers={'Authorization':'Bearer '+key})
        with opener.open(poll,timeout=8) as response:
            command=json.load(response).get('command')
        if command is not None:
            if not (command.get('kind')=='protect' and command.get('position')==777001
                    and command.get('magic')==410600 and command.get('expected_position_tp')==2510.0
                    and 2490.0<command.get('protect_stop',0)<2500.0):
                raise RuntimeError('Unexpected synthetic protection command')
            auth=urllib.request.Request('http://127.0.0.1:18741/v1/execution/authorize',
                data=json.dumps({'id':command['id']}).encode(),headers={'Authorization':'Bearer '+key,'Content-Type':'application/json'})
            with opener.open(auth,timeout=8) as response:
                if json.load(response).get('authorized') is not True:raise RuntimeError('Synthetic protection authorization failed')
            reply=urllib.request.Request('http://127.0.0.1:18741/v1/execution/result',
                data=json.dumps({'id':command['id'],'state':'placed','reason':'CI fake: no broker call'}).encode(),
                headers={'Authorization':'Bearer '+key,'Content-Type':'application/json'})
            with opener.open(reply,timeout=8) as response:
                if json.load(response).get('ok') is not True:raise RuntimeError('Synthetic protection result rejected')
            synthetic_stop=command['protect_stop']
    if not (home / 'synthetic-owned-position.flag').exists():
        synthetic_stop=None
    cancel_close=(home / 'synthetic-cancel-close.flag').exists()
    cancel_manual=(home / 'synthetic-cancel-manual.flag').exists()
    if cancel_close or cancel_manual:
        # Simulate a terminal *cancellation*, not an execution. Only the one
        # hard-coded fixture ticket is eligible, and no MT5 API is called.
        poll=urllib.request.Request('http://127.0.0.1:18741/v1/execution/gateway',
            headers={'Authorization':'Bearer '+key})
        with opener.open(poll,timeout=8) as response:
            command=json.load(response).get('command')
        if command is not None:
            valid_close=cancel_close and command.get('kind')=='close' and command.get('manual_close') is True and command.get('position')==777001
            valid_manual=cancel_manual and command.get('kind')=='open' and command.get('manual_open') is True and command.get('brain')=='manual' and command.get('magic')==410618
            if not (valid_close or valid_manual):
                raise RuntimeError('Unexpected synthetic gateway command')
            result=dict(id=command['id'],state='cancelled',reason='CI fixture: no broker order was sent')
            reply=urllib.request.Request('http://127.0.0.1:18741/v1/execution/result',
                data=json.dumps(result).encode(),headers={'Authorization':'Bearer '+key,'Content-Type':'application/json'})
            with opener.open(reply,timeout=8) as response:
                if json.load(response).get('ok') is not True:raise RuntimeError('Synthetic cancellation rejected')
    time.sleep(0.25)
