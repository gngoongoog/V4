"""P0-E offline Native EA V25 Portable release structural gate.

No MetaEditor invocation, MT5 login, broker connection or order submission.
A passing package check is NOT an EA compile or Tick-to-Core validation.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import sys

REQUIRED = (
    "mt5/Experts/BrainTradeSniperBridge.mq5",
    "Tools/Compile-Sniper-EA.ps1",
    "Compile-Sniper-EA.cmd",
    "NATIVE_EA_ACCEPTANCE_AR.md",
)
EA_MARKERS = (
    "#property strict", "CopyTicks(", "ACCOUNT_LOGIN", "ACCOUNT_SERVER",
    "ACCOUNT_CURRENCY", "ACCOUNT_TRADE_MODE", "g_connected_token != g_token",
    "ResetQuoteCursor();", "FileWriteString", "BT2|", "BrainTrade",
)
SCRIPT_MARKERS = (
    "param(", "MetaEditorPath", "TerminalDataDir", "InspectOnly",
    'metaeditor-compile.json', "LastWriteTimeUtc", "Get-FileHash",
    "ready_for_real", "NOT TESTED", "MetaEditor",
)
FORBIDDEN_EA = ("OrderSend(", "OrderSendAsync(", "CTrade", "trade.Buy", "trade.Sell")
FORBIDDEN_SCRIPTS = ("Invoke-RestMethod", "Invoke-WebRequest", "order_send(",
                     "/v1/start", "/v1/execution/authorize")


def verify(portable: Path) -> dict:
    portable=portable.resolve()
    checks={}
    for name in REQUIRED:
        path=portable/name
        checks["asset:"+name]={
            "status":"PASS" if path.is_file() and path.resolve().is_relative_to(portable)
                      else "FAIL",
            "reason":"present" if path.is_file() else "missing",
        }
    specs={
        REQUIRED[0]:(EA_MARKERS,FORBIDDEN_EA),
        REQUIRED[1]:(SCRIPT_MARKERS,FORBIDDEN_SCRIPTS),
        REQUIRED[2]:("Compile-Sniper-EA.ps1","-InspectOnly","powershell.exe"),
        REQUIRED[3]:("MetaEditor","DEMO","NOT TESTED"),
    }
    for name,markers in specs.items():
        path=portable/name
        missing=[]
        try:
            text=path.read_text(encoding="utf-8-sig")
            required,forbidden=markers if len(markers)==2 and isinstance(markers[0],tuple) else (markers,())
            missing=[value for value in required if value not in text]
            missing += ["forbidden:"+value for value in forbidden if value in text]
        except (OSError,UnicodeError):
            missing=["unreadable"]
        checks["contract:"+name]={
            "status":"FAIL" if missing else "PASS",
            "reason":", ".join(missing[:5]) if missing else "static inspection only",
        }
    return {
        "schema":"braintrade-v25-p0e-native-package-v1",
        "checks":checks,
        "ea_metaeditor_compile":"NOT TESTED",
        "native_pipe_windows_demo":"NOT TESTED",
        "on_tick_latency":"NOT TESTED",
        "broker_demo_lifecycle":"NOT TESTED",
        "ready_for_real":False,
        "note":"Static source/packaging only. MetaEditor, actual OnTick and broker acceptance require Windows MT5.",
    }


def main(argv=None)->int:
    parser=argparse.ArgumentParser(description="Offline V25 Native EA package/contract inspection")
    parser.add_argument("--portable",required=True,type=Path)
    parser.add_argument("--output",type=Path)
    args=parser.parse_args(argv)
    report=verify(args.portable)
    if args.output:
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    print(json.dumps({k:v["status"] for k,v in report["checks"].items()},indent=2))
    return int(any(row["status"]!="PASS" for row in report["checks"].values()))


if __name__=="__main__":
    sys.exit(main())
