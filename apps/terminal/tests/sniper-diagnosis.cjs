const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const ts=require('typescript');
const root=path.resolve(__dirname,'..');
const file=p=>fs.readFileSync(path.join(root,p),'utf8');
const ui=file('src/sniper-diagnosis.tsx');
const reconciliation=file('src/reconciliation-diagnostics.tsx');
const editor=file('src/operations.tsx');
const terminal=file('src/terminal.tsx');
const tauri=fs.readFileSync(path.join(root,'../tauri/src/bridge.rs'),'utf8');
const core=fs.readFileSync(path.join(root,'../../crates/runtime/src/lib.rs'),'utf8');

for(const name of ['src/sniper-diagnosis.tsx','src/reconciliation-diagnostics.tsx','src/operations.tsx']){
 const source=file(name);
 const result=ts.transpileModule(source,{fileName:name,compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX},reportDiagnostics:true});
 const errors=(result.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error);
 assert.equal(errors.length,0,errors.map(e=>ts.flattenDiagnosticMessageText(e.messageText,'\n')).join('\n'));
}
assert.match(tauri,/"\/v1\/execution\/sniper-diagnosis"/,'Tauri read-only allowlist');
assert.match(core,/sniper_opportunities::report\(&self\.db,&context\)/);
assert(core.indexOf('if method=="GET" && path=="/v1/execution/sniper-diagnosis"')<core.indexOf('if path.starts_with("/v1/execution")'),'Diagnostic route must precede generic execution router');
assert.match(core,/sniper_blocker_review::report\(&self\.db,&context,now_ms\(\)\)/);
assert.match(core,/t\.account_trade_mode==Some\(0\)/,'Demo must be verified');
assert.match(core,/"endpoint_submits_orders":false/);
assert.match(ui,/r\.context===identity/,'account scope must be verified in UI');
assert.match(ui,/setInterval\(\(\)=>\{void read\(\)\},7000\)/);
assert.match(ui,/setReport\(null\)/,'no stale diagnosis on failure');
assert.match(terminal,/<SniperDiagnosis key=\{viewScope\}/);
assert.match(terminal,/throw new Error\('القيم التي أعادها Core تختلف عن الإعدادات المطلوبة'\)/);
assert.match(terminal,/catch\(e\)\{if\(writeFresh\(mutation\)\)sendNotice\(String\(e\),true\);throw e\}/);
assert.match(editor,/catch\(e\)\{setError\('فشل الحفظ؛ تم الاحتفاظ بالتعديلات:/);
assert.match(editor,/payload\.account=account/);
for(const field of ['sniper_target_points','sniper_stop_points','sniper_target_mode',
 'sniper_cooldown_seconds','sniper_max_hold_seconds','sniper_min_target_pips',
 'sniper_max_target_pips','sniper_pip_price','max_cost_fraction',
 'demo_flexible','adaptive_exits']){
 assert(editor.includes(field),'Missing editable Sniper parameter '+field);
}
assert.match(tauri,/"\\/v1\\/execution\\/reconciliation"/,'Tauri read-only allowlist');
assert(core.includes('reconciliation_audit::report(&self.db,'),'Core uses read-only audit');
assert(core.indexOf('if method=="GET" && path=="/v1/execution/reconciliation"')<
 core.indexOf('if path.starts_with("/v1/execution")'));
assert.match(reconciliation,/readPanel\\('\\/v1\\/execution\\/reconciliation'\\)/);
assert.match(reconciliation,/current_session_pending/);
assert.match(reconciliation,/other_or_unverified_session_pending/);
assert(!reconciliation.includes('readPanel("/v1/gateway/snapshot"'));
assert.match(terminal,/<ReconciliationDiagnostics key=\\{viewScope\\}/);
console.log('PASS: Sniper save contract, account-scoped diagnostic, read-only enforcement and draft retention');
