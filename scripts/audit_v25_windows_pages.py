"""P0-D: inspect all 22 production React/Tauri pages and allowed local Core routes.

A static source contract, not WebView2 mouse-click acceptance. It fails closed on
missing/duplicate pages, unknown read routes, unwired Tauri invocation, stale
page renderer fences, or unsafe browsing around the Core IPC proxy.
"""
from __future__ import annotations
import argparse
from collections import Counter
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import sys

ROOT=Path(__file__).resolve().parents[1]
EXPECTED=(
    "dashboard", "trading", "execution", "brains", "brainControl", "sniper",
    "risk", "positions", "analysis", "journal", "reports", "telegram",
    "settings", "shadow", "learning", "backtest", "evaluation", "correlation",
    "studio", "news", "backups", "events",
)
FEATURED=("risk", "execution", "trading", "sniper", "backtest",
          "backups", "positions", "brains", "brainControl", "settings", "telegram")
APPROVED_COMMANDS=(
    "execute_control", "update_risk", "update_portfolio", "update_settings",
    "priority_handoff", "shutdown_core", "close_terminal", "backup_action",
    "brain_snapshot_export", "brain_snapshot_import", "brain_snapshot_restore",
    "backtest_action", "update_trading_settings", "update_brain_control",
    "bind_execution", "update_supervisor", "frozen_coordinator_action", "frozen_sniper_action", "frozen_classic_action",
    "read_panel", "read_trade_history", "get_status",
    "subscribe_status",
)


def check(root:Path=ROOT)->dict:
    terminal=(root/"apps/terminal/src/terminal.tsx").read_text("utf-8")
    types=(root/"apps/terminal/src/types.ts").read_text("utf-8")
    bridge=(root/"apps/terminal/src/bridge.ts").read_text("utf-8")
    tauri=(root/"apps/tauri/src/bridge.rs").read_text("utf-8")
    page_block=terminal.split("const PAGES:",1)[1].split("];",1)[0]
    rows=re.findall(r"\{id:'([^']+)',title:'([^']+)',en:'([^']+)',icon:\w+(?:,route:'([^']+)')?\}",page_block)
    ids=[item[0] for item in rows]
    route_by_page={item[0]:item[3] or None for item in rows}
    allowed_section=tauri.split("fn allowed_read_route(",1)[1].split("#[tauri::command]",1)[0]
    allowed_routes=set(re.findall(r'"(/v1/[a-z0-9/-]+)"',allowed_section))
    typeblock=types.split("export type PageId=",1)[1].split(";",1)[0]
    type_ids=set(re.findall(r"'([^']+)'",typeblock))
    declared_commands=set(re.findall(r"invoke(?:<[^>]+>)?\('([^']+)'",bridge))
    registered=tauri.split("tauri::generate_handler![",1)[1].split("]",1)[0]
    primary_match=re.search(r"const primaryNav:PageId\[\]=\[([^\]]+)\]",terminal)
    primary_ids=re.findall(r"'([^']+)'",primary_match.group(1)) if primary_match else []
    checks={}
    def add(name:str,passed:bool,detail:str):
        checks[name]={"status":"PASS" if passed else "FAIL","detail":detail}
    add("22_unique_page_registry",
        len(ids)==22 and len(set(ids))==22 and set(ids)==set(EXPECTED),
        f"22 expected; found {len(ids)}; repeated={list(k for k,v in Counter(ids).items() if v>1)}")
    add("typed_page_ids_match_registry",type_ids==set(ids),
        "PageId union and production navigation IDs agree")
    add("all_pages_have_allowed_tauri_core_route",
        all((page=="dashboard" and route is None)
            or (page!="dashboard" and route in allowed_routes)
            for page,route in route_by_page.items()),
        "Dashboard uses live status; all other pages must read allowlisted Core GET routes")
    add("navigation_actions_for_all_pages",
        bool(primary_ids) and len(primary_ids)==len(set(primary_ids))
        and set(primary_ids).issubset(set(ids))
        and "sidebarPages=[...primaryNav.map(findPage),...PAGES.filter(x=>!primaryNav.includes(x.id))]" in terminal
        and "sidebarPages.filter((x,i)=>allOpen||i<11||x.id===page)" in terminal
        and "!allOpen&&!primaryNav.includes(x.id)&&x.id!==page" in terminal
        and "onClick={()=>open(x.id)}" in terminal
        and "aria-current={page===x.id?'page':undefined}" in terminal,
        "All 22 pages reachable; collapsed navigation retains the active page")
    add("source_scoped_render_fence",
        "pagePanel?.scope===viewScope?pagePanel.data:null" in terminal
        and "setPagePanel({scope:viewScope,data})" in terminal
        and "viewGeneration.current.isCurrent(serial)" in terminal,
        "Old account/page data hidden before async effects execute")
    add("live_data_and_preview_separated",
        "preview?DESIGN_FIXTURE:status" in terminal
        and "if(page==='dashboard'||!hasTauri()||preview)" in terminal
        and "feedEvidence(status,online,lastStatusAt,Date.now())" in terminal,
        "Mock display isolated from Windows live data")
    add("render_or_explore_all_22",
        all(f"page==='{id}'" in terminal for id in FEATURED)
        and "<DataExplorer source={pageData}" in terminal
        and "page==='dashboard'?" in terminal,
        "Specialist pages use domain views; others use read-only Data Explorer")
    add("no_raw_browser_core_http_or_secrets",
        "ipc.key" not in bridge and "http://" not in bridge
        and "allowed_read_route(&route)" in tauri
        and "readPanel(route)" in terminal,
        "Browser communicates via scoped Tauri IPC, not Core HTTP or token files")
    add("all_frontend_actions_registered_in_tauri",
        declared_commands.issubset(set(re.findall(r"\b([a-z_]+)\b",registered)))
        and set(APPROVED_COMMANDS).issuperset(declared_commands),
        "Every Tauri UI invoke has a registered handler and is in the bounded command inventory")
    return {
        "schema":"braintrade-v25-p0d-source-matrix-v1",
        "observed_utc":datetime.now(timezone.utc).isoformat(),
        "pages":[{"id":item[0],"title_ar":item[1],"title_en":item[2],
                  "core_route":item[3] or None,
                  "route_contract":"PASS" if (item[0]=="dashboard" and not item[3])
                    or item[3] in allowed_routes else "FAIL"} for item in rows],
        "checks":checks,
        "windows_webview2_clicks":"NOT TESTED",
        "22_page_visual_review":"NOT TESTED",
        "mt5_demo":"NOT TESTED",
        "real_trading_ready":False,
        "note":"Static source/IPC tests cannot certify runtime WebView2 clicks, chart rendering, or broker behavior.",
    }


def main(argv=None)->int:
    parser=argparse.ArgumentParser(description="Offline V25 Windows 22-page source-route and action contract audit")
    parser.add_argument("--root",type=Path,default=ROOT)
    parser.add_argument("--output",type=Path)
    args=parser.parse_args(argv)
    report=check(args.root)
    if args.output:
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    print(json.dumps({k:v["status"] for k,v in report["checks"].items()},indent=2))
    return int(any(x["status"]!="PASS" for x in report["checks"].values()))


if __name__=="__main__":
    sys.exit(main())
