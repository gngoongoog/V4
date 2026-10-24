// P0-D broker/account-scoped confirmations: executable TS logic + terminal wiring.
// Does not invoke Tauri, Core, MT5, or any live trading action.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const ts=require('typescript');
const source=fs.readFileSync(path.join(__dirname,'../src/operator-approval.ts'),'utf8');
const compiled=ts.transpileModule(source,{fileName:'operator-approval.ts',
 compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
 reportDiagnostics:true});
assert.equal((compiled.diagnostics||[]).filter(x=>x.category===ts.DiagnosticCategory.Error).length,0);
const result={exports:{}};vm.runInNewContext(compiled.outputText,{exports:result.exports});
const {approvalIsCurrent,canConfirmStart}=result.exports;
const scope='account:101:D-Server:XAUUSD:USD:0:true:true';
const approval={action:'start',scope};
const demo={gateway_fresh:true,requested_run:false,
 telemetry:{connected:true,demo:true,account_trade_mode:0,login:101,
 server:'D-Server',currency:'USD',symbol:'XAUUSD'}};
assert(canConfirmStart(approval,scope,demo,true,''));
assert(!canConfirmStart(approval,scope,demo,true,'101'),'Demo must not accept an unrelated REAL confirmation');
assert(!canConfirmStart(approval,scope,demo,false,''),'disconnected broker denies Start');
assert(!canConfirmStart(approval,'account:102',demo,true,''),'scope change revokes old consent');
assert(!canConfirmStart({action:'stop',scope},scope,demo,true,''),'wrong action never confirms Start');
assert(!canConfirmStart(null,scope,demo,true,''),'no dialog no consent');
assert(!canConfirmStart(approval,scope,{...demo,requested_run:true},true,''));
assert(!canConfirmStart(approval,scope,{...demo,gateway_fresh:false},true,''));
for(const mismatch of [
 {demo:false,account_trade_mode:0},{demo:true,account_trade_mode:2},
 {demo:true,account_trade_mode:null},{connected:false},{login:0},{login:NaN},
 {server:''},{symbol:''},{currency:''}
]){
 assert(!canConfirmStart(approval,scope,{...demo,telemetry:{...demo.telemetry,...mismatch}},true,''),
 'inconsistent or absent broker identity must fail: '+JSON.stringify(mismatch));
}
const real={...demo,telemetry:{...demo.telemetry,demo:false,account_trade_mode:2,login:34917}};
assert(!canConfirmStart(approval,scope,real,true,''));
assert(!canConfirmStart(approval,scope,real,true,'34918'));
assert(!canConfirmStart(approval,scope,real,true,'34917x'));
assert(canConfirmStart(approval,scope,real,true,' 34917 '));
assert(approvalIsCurrent({action:'exit',scope},'exit',scope));
assert(!approvalIsCurrent({action:'exit',scope},'exit','switched'));
const ui=fs.readFileSync(path.join(__dirname,'../src/terminal.tsx'),'utf8');
for(const token of [
 "const showStartConfirm=approvalIsCurrent(approval,'start',contextKey)",
 "const showStopConfirm=approvalIsCurrent(approval,'stop',contextKey)",
 "const showExitConfirm=approvalIsCurrent(approval,'exit',contextKey)",
 "setApproval({action:'start',scope:contextKey})",
 "setApproval({action:'stop',scope:contextKey})",
 "setApproval({action:'exit',scope:contextKey})",
 "if(action==='start'&&!canConfirmStart(",
 "if(!approvalIsCurrent(approval,'stop',contextKey))",
 "if(preview||!hasTauri()||!approvalIsCurrent(approval,'exit',contextKey))",
 "disabled={busy||!canConfirmStart(approval,contextKey,status,feed.broker,liveLogin)}",
 "event.key==='Escape'", "event.key!=='Tab'",
 "role=\"dialog\" aria-modal=\"true\" aria-label=\"تأكيد تشغيل التداول\"",
]){
 assert(ui.includes(token),'missing dialog/account guard: '+token);
}
for(const token of [
 "const showStartConfirm=approvalIsCurrent(approval,'start',contextKey)&&!preview&&feed.broker",
 "if(approval?.action==='start'&&(!feed.broker||approval.scope!==contextKey||preview))",
 "const safeConfig=configScope===contextKey?config:null",
 "const safeRiskSnapshot=riskScope===contextKey?riskSnapshot:null",
 "const activeSettingsEdit=settingsScope===contextKey?settingsEdit:null",
 "const activeTelegramToken=!preview&&telegramDraftScope===contextKey?telegramToken:''",
 "const activeClearTelegram=!preview&&telegramDraftScope===contextKey?clearTelegram:false",
 "await saveSettings(activeSettingsEdit,activeTelegramToken,activeClearTelegram)",
 "disabled={busy||preview||!activeSettingsEdit}",
 "key={contextKey} config={safeConfig?",
 "setTelegramToken('');setClearTelegram(false);setTelegramDraftScope(null);",
 "setApproval(null);setLiveLogin('');",
]){
 assert(ui.includes(token),'account-scoped read-only editor/secret regression: '+token);
}
assert(!ui.includes('setShowStartConfirm('),'legacy boolean approval must be removed');
assert(!ui.includes('setShowStopConfirm('),'legacy boolean approval must be removed');
assert(!ui.includes('setShowExitConfirm('),'legacy boolean approval must be removed');
console.log('PASS: broker scope, Demo mode, exact REAL login, connection loss, cancel/stop/exit and accessible confirmation interactions');
