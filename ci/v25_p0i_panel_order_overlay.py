"""P0-I: reject a stale result even when TWO reads target the SAME page/account.

Installs the single tested TS generation fence (ci/v25_p0i_panel_guard.ts)
into Tauri UI after the P0-H overlay. Exact unique source anchors only.
"""
from pathlib import Path

ui=Path("apps/terminal/src/terminal.tsx")
s=ui.read_text(encoding="utf-8")
def one(old,new,label):
    global s
    if s.count(old)!=1:
        raise SystemExit(f"P0-I unexpected Tauri UI anchor {label} ({s.count(old)} matches)")
    s=s.replace(old,new,1)

one("import TradingChart from './chart';",
    "import TradingChart from './chart';\nimport {PanelReadFence} from './panel-guard';","import")
one(""" const viewGeneration=useRef({scope:'',serial:0});
 const viewScope=JSON.stringify([page,contextKey,preview]);
 if(viewGeneration.current.scope!==viewScope){viewGeneration.current={scope:viewScope,serial:viewGeneration.current.serial+1}}
 const panelSerial=()=>viewGeneration.current.serial;""",
    """ const viewGeneration=useRef(new PanelReadFence());
 const viewScope=JSON.stringify([page,contextKey,preview]);
 viewGeneration.current.setScope(viewScope);
 const panelSerial=()=>viewGeneration.current.beginRead();""","request sequence")
old="if(viewGeneration.current.serial!==serial)return;"
if s.count(old)!=2:
    raise SystemExit("P0-I current-result guards unexpectedly modified")
s=s.replace(old,"if(!viewGeneration.current.isCurrent(serial))return;")
one("""    try{const data=await readPanel(route);if(active){setPageData(data);setPageUpdatedAt(Date.now());setPageError('')}}
    catch(e){if(active){setPageData(null);setPageUpdatedAt(null);setPageError(String(e))}}""",
    """    const serial=panelSerial();
    try{const data=await readPanel(route);if(active)publishPanel(serial,data)}
    catch(e){if(active)rejectPanel(serial,e)}""","periodic panel read")
one("publishPanel(serial,latest);if(viewGeneration.current.serial===serial)setConfig(full);",
    "publishPanel(serial,latest);if(viewGeneration.current.isCurrent(serial))setConfig(full);",
    "trading settings account guard")
ui.write_text(s,encoding="utf-8")

module=Path("apps/terminal/src/panel-guard.ts")
if module.exists():
    raise SystemExit("P0-I existing TS fence unexpected; refusing accidental overwrite")
module.write_bytes(Path("ci/v25_p0i_panel_guard.ts").read_bytes())

contracts=Path("apps/terminal/tests/contracts.cjs")
s=contracts.read_text(encoding="utf-8")
for old,new in [
  ('assert.match(terminal,/const viewGeneration=useRef\\(/);',
   'assert.match(terminal,/const viewGeneration=useRef\\(new PanelReadFence\\(\\)\\)/);'),
  ('assert.match(terminal,/viewGeneration\\.current\\.scope!==viewScope/);',
   'assert.match(terminal,/viewGeneration\\.current\\.setScope\\(viewScope\\)/);'),
  ('assert.match(terminal,/if\\(viewGeneration\\.current\\.serial!==serial\\)return;/);',
   'assert.match(terminal,/if\\(!viewGeneration\\.current\\.isCurrent\\(serial\\)\\)return;/);')
]:
    if s.count(old)!=1:
        raise SystemExit(f"P0-I Node contract drift: {old}")
    s=s.replace(old,new)
contracts.write_text(s,encoding="utf-8")
print("P0-I newest-wins account-scoped panel fetch protection installed")
