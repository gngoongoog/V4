"""Core API acceptance for the exclusive Sniper-primary/backup route.

Runs only against a temporary locally launched Core and synthetic MT5 snapshots.
Never connects to a real broker and never sends a trade.
"""
from __future__ import annotations
import argparse, json, os, subprocess, sys, tempfile, time, urllib.request, urllib.error
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT / 'gateway'))
from mt5_gateway import empty_snapshot

def check(exe=None):
    executable = Path(exe or ROOT / "target/debug/braintrade-core").resolve()
    command = [str(executable)] + (["--core"] if exe else [])
    with tempfile.TemporaryDirectory(prefix="braintrade-priority-") as temp:
        environment=dict(os.environ,
            BRAINTRADE_DATA_DIR=temp,
            BRAINTRADE_GATEWAY_DISABLED="1",
            BRAINTRADE_NEWS_OFFLINE="1")
        opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
        key=""
        log=open(Path(temp)/"core.log","w+b")
        core=None

        def start():
            nonlocal core,key
            core=subprocess.Popen(command,env=environment,stdout=log,stderr=log)
            for _ in range(140):
                if core.poll() is not None: break
                try:
                    if os.name=="nt":
                        from check_windows_portable import unseal
                        key=unseal(Path(temp)/"ipc.key")
                    else:
                        key=Path(temp,"ipc.key").read_text()
                    api("/v1/status")
                    return
                except OSError:
                    time.sleep(.1)
            raise AssertionError("Core did not start")

        def api(path,payload=None):
            body=None if payload is None else json.dumps(payload).encode("utf-8")
            request=urllib.request.Request("http://127.0.0.1:18741"+path,
                data=body,headers={"Authorization":"Bearer "+key,
                                   "Content-Type":"application/json"})
            with opener.open(request,timeout=10) as response:
                return json.load(response)

        def refuses(path,payload):
            try:api(path,payload)
            except urllib.error.HTTPError as error:
                assert error.code==400,(path,error.code,error.read().decode())
            else:raise AssertionError("Unsafe handoff unexpectedly accepted")

        snap=empty_snapshot("")
        snap.update(connected=True,demo=True,account_trade_mode=0,
            login=17,server="Demo",currency="USD",symbol="XAUUSD",
            trade_allowed=True,trade_expert=True,
            balance=10000.0,equity=10000.0,today_net=0.,
            point=.01,tick_size=.01,tick_value_loss=1.,
            tick_value_profit=1.,volume_min=.01,volume_step=.01,
            volume_max=100.,bid=2340.0,ask=2340.1)
        def publish():
            snap.update(quote_time_ms=int(time.time()*1000),tick_age_ms=0)
            api("/v1/gateway/snapshot",snap)
        try:
            start()
            baseline=api("/v1/trading/settings")["config"]
            assert not baseline["trading"]["priority_enabled"]
            refuses("/v1/trading/priority/handoff",{"sniper_active":False})
            publish()
            api("/v1/execution/settings",{"target":"demo","deviation_points":20})
            for backup in ("scalping","swing","expert"):
                config=api("/v1/trading/settings")["config"]
                strategy=dict(config["trading"],priority_enabled=True,
                    sniper_active=True,fallback_mode=backup,mode="sniper")
                saved=api("/v1/trading/settings",{"trading":strategy,
                    "commission_per_lot":7.0})
                assert saved["priority"]["active_mode"]=="sniper"
                assert saved["priority"]["fallback_mode"]==backup
                rejects=dict(strategy,fallback_mode="sniper")
                refuses("/v1/trading/settings",{"trading":rejects,"commission_per_lot":7.0})
                publish()
                api("/v1/start",{})
                assert api("/v1/status")["requested_run"]
                refuses("/v1/trading/settings",{"trading":strategy,"commission_per_lot":7.0})
                refuses("/v1/trading/priority/handoff",{"sniper_active":"false"})

                # Fail closed with stale quotes, open positions or MT5 pending orders.
                snap["tick_age_ms"]=9000
                api("/v1/gateway/snapshot",snap)
                refuses("/v1/trading/priority/handoff",{"sniper_active":False})
                publish()
                snap["pending_orders"]=1
                publish()
                refuses("/v1/trading/priority/handoff",{"sniper_active":False})
                snap["pending_orders"]=0
                snap["positions"]=[{"ticket":441,"magic":0,
                    "symbol":"XAUUSD","side":"BUY","volume":0.01,
                    "price_open":2340.0,"price_current":2340.0,
                    "profit":0.0,"sl":2335.0,"tp":2350.0}]
                publish()
                refuses("/v1/trading/priority/handoff",{"sniper_active":False})
                snap["positions"]=[]
                publish()
                switched=api("/v1/trading/priority/handoff",{"sniper_active":False})
                assert switched["priority"]["active_mode"]==backup
                assert not switched["priority"]["sniper_active"]
                assert api("/v1/status")["requested_run"],"handoff must preserve active monitoring"
                assert api("/v1/config")["config"]["trading"]["mode"]==backup
                refuses("/v1/trading/priority/handoff",{"sniper_active":False})
                publish()
                restored=api("/v1/trading/priority/handoff",{"sniper_active":True})
                assert restored["priority"]["active_mode"]=="sniper"
                assert restored["priority"]["sniper_active"]
                assert api("/v1/status")["requested_run"]
                api("/v1/stop",{})
            # Persistence and legacy config compatibility are independent of UI state.
            before=api("/v1/config")["config"]["trading"]
            assert before["priority_enabled"] and before["fallback_mode"]=="expert"
            api("/v1/shutdown",{})
            core.wait(timeout=8)
            start()
            after=api("/v1/config")["config"]["trading"]
            assert after==before,(before,after)
            assert not api("/v1/status")["requested_run"]
            print("PASS: exclusive Sniper priority, all 3 backups, live monitoring handoff, no positions/orders/stale quotes, persistence and restart STOPPED")
            api("/v1/shutdown",{})
            core.wait(timeout=8)
        finally:
            if core and core.poll() is None:
                core.kill()
                core.wait(timeout=8)
            log.close()

if __name__=="__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--exe",default=None)
    args=parser.parse_args()
    check(args.exe)
