const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const src=fs.readFileSync(path.join(__dirname,'../src/terminal.tsx'),'utf8');
const binding=fs.readFileSync(path.join(__dirname,'../src/operations.tsx'),'utf8');
const bridge=fs.readFileSync(path.join(__dirname,'../src/bridge.ts'),'utf8');
const core=fs.readFileSync(path.join(__dirname,'../../../crates/runtime/src/execution_legacy.rs'),'utf8');
const router=fs.readFileSync(path.join(__dirname,'../../../crates/runtime/src/execution.rs'),'utf8');
const tauri=fs.readFileSync(path.join(__dirname,'../../tauri/src/bridge.rs'),'utf8');
const header=src.match(/<div className="account-item account-switch">[\s\S]*?<\/div>\s*<\/div>/);
assert(header,'A visible operator account-selection toolbar must exist');
assert(header[0].includes('MT5: ')&&header[0].includes('Core: '),
 'Never describe a Core-selected target as if the MT5 broker had switched accounts');
assert(header[0].includes("onClick={()=>quickSelectAccount('demo')}"),
 'Demo button must invoke the actual Core binding procedure');
assert(header[0].includes("onClick={()=>quickSelectAccount('live')}"),
 'Live button must invoke the actual Core binding procedure');
assert(header[0].includes("aria-pressed={verifiedBinding&&boundTarget==='live'}"),
 'Only verified bound MT5 live settings may show Live as selected');
assert(src.includes("void runBrokerBind(target,deviationPoints,confirmLiveLogin)"));
assert(src.includes("const committed=unwrap(await bindExecution(target,deviationPoints,confirmLiveLogin))"));
assert(src.includes("committed.ok!==true||unwrap(committed.settings).target!==target"),
 'UI may not announce success without Core acknowledging the selected target');
assert(src.includes("if(page==='execution')publishPanel(serial,refreshed)"),
 'A global account binding must not publish execution data into any other page');
assert(src.includes('اكتب رقم حساب MT5 المعروض'),
 'Live button must require explicit operator confirmation of broker login');
assert(src.includes('سجّل دخول حساب '),
 'Clicking the switch may not claim to log MT5 into a different account');
assert(tauri.includes('"/v1/execution/binding"'), 'Tauri must allow the binding read-only endpoint');
assert(router.includes('path=="/v1/execution/binding"'), 'Core must return a dedicated immutable account-binding state');
assert(src.includes("readPanel('/v1/execution/binding')"),
 'Every page must read the source-of-truth Core binding directly');
assert(src.includes("bindingSlot?.scope===contextKey"),'Do not show a prior account binding after switching accounts');
assert(src.includes("binding.connected_target!==target"),'Do not announce a bind success if Core sees a different broker account');
assert(src.includes("bindingSnapshot.pending_reconciliation===true"),
 'Account switch must recognize unresolved broker intent');
assert(src.includes('status.requested_run'),
 'Account binding must not be exposed as a hot swap during START');
assert(binding.includes("saved.target==='live'?'live':saved.target==='demo'?'demo'"),
 'Binding form must show the persisted Core selection rather than default Demo');
assert(binding.includes("telemetry.account_trade_mode!==2||login.trim()!==String(telemetry.login)"),
 'A live account must be broker-verified and typed by the operator');
assert(binding.includes("onBind(target,points,login.trim())"));
assert(bridge.includes("invoke('bind_execution',{target,deviationPoints,confirmLiveLogin})"));
assert(core.includes('status.requested_run || self.pending(db)?'),
 'Core must not change account while an order is pending or reconciliation incomplete');
assert(core.includes('v["confirm_live_login"].as_str()==Some(t.login.to_string().as_str())'),
 'Core must independently validate Live confirmation, not trust the UI');
console.log('PASS: visible Demo/Live buttons bind only exact MT5 account through Core, confirm Live identity and preserve page isolation');
