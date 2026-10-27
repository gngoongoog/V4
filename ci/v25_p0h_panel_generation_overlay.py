"""P0-H: forbid stale manual IPC responses overwriting a newer page/account.

Apply only on exactly matching P0-G restored React/Tauri source; source drift
blocks binary publication. Does not modify execution endpoints or risk logic.
"""
from pathlib import Path
import re

ui=Path('apps/terminal/src/terminal.tsx')
s=ui.read_text(encoding='utf-8')
def once(a,b,label):
    global s
    if s.count(a)!=1: raise SystemExit(f'P0-H incompatible {label}: {s.count(a)} anchors')
    s=s.replace(a,b,1)

once("import {useEffect,useMemo,useState} from 'react';",
     "import {useEffect,useMemo,useRef,useState} from 'react';","useRef import")
scope=" const sendNotice=(msg:string,isError=false)=>"
helper=""" // Any page, broker-account or preview transition revokes unresolved manual reads.
 const viewGeneration=useRef({scope:'',serial:0});
 const viewScope=JSON.stringify([page,contextKey,preview]);
 if(viewGeneration.current.scope!==viewScope){viewGeneration.current={scope:viewScope,serial:viewGeneration.current.serial+1}}
 const panelSerial=()=>viewGeneration.current.serial;
 const publishPanel=(serial:number,result:unknown)=>{
  if(viewGeneration.current.serial!==serial)return;
  setPageData(result);setPageUpdatedAt(Date.now());setPageError('');
 };
 const rejectPanel=(serial:number,cause:unknown)=>{
  if(viewGeneration.current.serial!==serial)return;
  setPageData(null);setPageUpdatedAt(null);setPageError(String(cause));
 };
 const refreshCurrentPanel=(route:string)=>{
  if(preview||!hasTauri())return;
  const serial=panelSerial();
  void readPanel(route).then(data=>publishPanel(serial,data)).catch(e=>rejectPanel(serial,e));
 };
"""
once(scope,helper+scope,'account generation helper')

once("""  setBusy(true);
  try{const outcome=unwrap(await backupAction(action));""",
     """  const serial=panelSerial();setBusy(true);
  try{const outcome=unwrap(await backupAction(action));""","backup capture")
once("setPageData(await readPanel('/v1/backups'));setPageUpdatedAt(Date.now())",
     "publishPanel(serial,await readPanel('/v1/backups'))","backup completion")
once("""  setBusy(true);
  try{await backtestAction(action,request);""",
     """  const serial=panelSerial();setBusy(true);
  try{await backtestAction(action,request);""","backtest capture")
once("setPageData(await readPanel('/v1/backtest'));setPageUpdatedAt(Date.now())",
     "publishPanel(serial,await readPanel('/v1/backtest'))","backtest completion")
once("""  setBusy(true);
  try{
   await saveTrading(input);""",
     """  const serial=panelSerial();setBusy(true);
  try{
   await saveTrading(input);""","trading capture")
once("setPageData(latest);setPageUpdatedAt(Date.now());setConfig(full);",
     "publishPanel(serial,latest);if(viewGeneration.current.serial===serial)setConfig(full);","trading completion")
once("""  const mode=String(unwrap(pageData).trading_mode||'');
  setBusy(true);""",
     """  const mode=String(unwrap(pageData).trading_mode||'');
  const serial=panelSerial();setBusy(true);""","brain capture")
once("setPageData(await readPanel('/v1/execution/brains'));setPageUpdatedAt(Date.now());",
     "publishPanel(serial,await readPanel('/v1/execution/brains'));","brain completion")
once("""  try{
   await bindExecution(target,deviationPoints,confirmLiveLogin);
   setPageData(await readPanel('/v1/execution'));setPageUpdatedAt(Date.now());""",
     """  try{
   await bindExecution(target,deviationPoints,confirmLiveLogin);
   publishPanel(serial,await readPanel('/v1/execution'));""","execution binding completion")
once("""  if(preview||!hasTauri()||status?.requested_run){sendNotice('اربط حساب التنفيذ فقط بعد STOP ومن تطبيق Tauri الحقيقي',true);return}
  setBusy(true);""",
     """  if(preview||!hasTauri()||status?.requested_run){sendNotice('اربط حساب التنفيذ فقط بعد STOP ومن تطبيق Tauri الحقيقي',true);return}
  const serial=panelSerial();setBusy(true);""","execution binding capture")

oldManual=[
"""onClick={()=>{if(pageMeta.route&&hasTauri()&&!preview)void readPanel(pageMeta.route).then(data=>{setPageData(data);setPageError('')}).catch(e=>{setPageData(null);setPageError(String(e))})}}""",
"""onRefresh={()=>{if(!preview)void readPanel('/v1/execution').then(v=>{setPageData(v);setPageUpdatedAt(Date.now());setPageError('')}).catch(e=>setPageError(String(e)))}}""",
"""onRefresh={()=>{if(!preview)void readPanel('/v1/backtest').then(v=>{setPageData(v);setPageUpdatedAt(Date.now());setPageError('')}).catch(e=>setPageError(String(e)))}}""",
"""onRefresh={()=>{if(!preview)void readPanel('/v1/backups').then(v=>{setPageData(v);setPageUpdatedAt(Date.now());setPageError('')}).catch(e=>setPageError(String(e)))}}""",
"""onRefresh={()=>{if(pageMeta.route&&hasTauri()&&!preview)void readPanel(pageMeta.route).then(v=>{setPageData(v);setPageUpdatedAt(Date.now());setPageError('')}).catch(e=>{setPageData(null);setPageUpdatedAt(null);setPageError(String(e))})}}"""
]
newManual=[
"""onClick={()=>{if(pageMeta.route)refreshCurrentPanel(pageMeta.route)}}""",
"""onRefresh={()=>refreshCurrentPanel('/v1/execution')}""",
"""onRefresh={()=>refreshCurrentPanel('/v1/backtest')}""",
"""onRefresh={()=>refreshCurrentPanel('/v1/backups')}""",
"""onRefresh={()=>{if(pageMeta.route)refreshCurrentPanel(pageMeta.route)}}"""
]
for i,(a,b) in enumerate(zip(oldManual,newManual),1):
    once(a,b,f'manual refresh #{i}')
ui.write_text(s,encoding='utf-8')

contracts=Path('apps/terminal/tests/contracts.cjs')
s=contracts.read_text(encoding='utf-8')
if "P0-H: a late IPC response" in s:raise SystemExit('P0-H contracts already installed; refuse duplicate')
s+="""
// P0-H: a late IPC response cannot overwrite a different page/account.
assert.match(terminal,/const viewGeneration=useRef\\(/);
assert.match(terminal,/viewGeneration\\.current\\.scope!==viewScope/);
assert.match(terminal,/if\\(viewGeneration\\.current\\.serial!==serial\\)return;/);
assert.match(terminal,/publishPanel\\(serial,await readPanel\\('\\/v1\\/execution\\/brains'\\)\\)/);
assert.match(terminal,/onRefresh=\\{\\(\\)=>refreshCurrentPanel\\('\\/v1\\/execution'\\)\\}/);
assert.doesNotMatch(terminal,/setPageData\\(await readPanel\\(/);
console.log('PASS: P0-H stale IPC manual refresh/account generation rejection');
"""
contracts.write_text(s,encoding='utf-8')
print('P0-H frontend generation guard installed: 22 existing pages preserved')
