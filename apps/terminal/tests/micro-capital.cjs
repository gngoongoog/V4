const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require('typescript'),assert=require('node:assert/strict');
const src=fs.readFileSync(path.join(__dirname,'../src/micro-capital.ts'),'utf8');
const js=ts.transpileModule(src,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});
const loaded={};vm.runInNewContext(js.outputText,{exports:loaded});
const {smallAccountPreview,validatePortfolio}=loaded;
const t={connected:true,demo:false,account_trade_mode:2,equity:50,balance:50,currency:'USD',leverage:500,
 margin_free:45,minimum_lot_margin:12,volume_min:0.01};
const risk={risk_percent:0.25,micro_real_temporary:true,micro_real_cap_percent:10,micro_real_recovery_equity:500};
const view=smallAccountPreview({gateway_fresh:true,telemetry:t},risk,100);
assert.equal(view.available,true);
assert.equal(view.accountType,'REAL');
assert.equal(view.equity,50);
assert.equal(view.leverage,500,'real MT5 leverage must never be replaced by scenario');
assert.equal(view.minimumLot,0.01);
assert.equal(view.minimumLotMargin,12);
assert.equal(view.scenarioMargin,60,'scenario 1:100 is indicative only');
assert.equal(view.marginCovered,true);
assert.equal(view.microCandidate,true);
assert.equal(view.budgetPercent,10);
assert.equal(view.budget,5);
const lev=smallAccountPreview({gateway_fresh:true,telemetry:t},risk,500);
assert.equal(lev.scenarioMargin,12,'same leverage preserves indicative broker minimum margin');
const omitted=smallAccountPreview({gateway_fresh:true,telemetry:t},
 {risk_percent:0.25,micro_real_temporary:true},500);
assert.equal(omitted.microCandidate,true,'serde omits default recovery milestone of 500');
assert.equal(omitted.budgetPercent,10,'serde omits default Micro cap of 10%');
assert.equal(omitted.budget,5,'50 USD micro default remains 5 USD estimated risk');
const unconfigured=smallAccountPreview({gateway_fresh:true,telemetry:t},null,500);
assert.equal(unconfigured.budget,null,'missing Core risk config is not interpreted as default');
const fifty=smallAccountPreview({gateway_fresh:true,telemetry:t},{...risk,micro_real_cap_percent:20},500);
assert.equal(fifty.budget,10,'20% micro is a high-risk estimate, never an order authorization');
const a100=smallAccountPreview({gateway_fresh:true,telemetry:{...t,equity:100}},risk,500);
assert.equal(a100.budget,10,'user can choose account 100 without a configured minimum deposit');
const recovered=smallAccountPreview({gateway_fresh:true,telemetry:{...t,equity:501}},risk,500);
assert.equal(recovered.microCandidate,false);
assert.equal(recovered.budget,1.2525,'falls back to normal per trade policy after threshold');
const notEnabled=smallAccountPreview({gateway_fresh:true,telemetry:t},{...risk,micro_real_temporary:false},500);
assert.equal(notEnabled.budget,0.125);
const demo=smallAccountPreview({gateway_fresh:true,telemetry:{...t,account_trade_mode:0,demo:true}},risk,500);
assert.equal(demo.microCandidate,false,'REAL opt-in can never bleed into Demo');
assert.equal(demo.budget,0.125);
for(const status of [
 null,
 {gateway_fresh:false,telemetry:t},
 {gateway_fresh:true,telemetry:{...t,connected:false}},
 {gateway_fresh:true,telemetry:{...t,demo:true,account_trade_mode:2}},
 {gateway_fresh:true,telemetry:{...t,equity:0}},
 {gateway_fresh:true,telemetry:{...t,equity:NaN}},
]){
 const value=smallAccountPreview(status,risk,500);
 assert.equal(value.available,false);
 assert.equal(value.budget,null,'unverified state must not synthesize a positive trade budget');
}
const noMargin=smallAccountPreview({gateway_fresh:true,telemetry:{...t,minimum_lot_margin:null,leverage:null}},risk,500);
assert.equal(noMargin.scenarioMargin,null);assert.equal(noMargin.marginCovered,null);
const insufficient=smallAccountPreview({gateway_fresh:true,telemetry:{...t,margin_free:5}},risk,500);
assert.equal(insufficient.marginCovered,false);
const config={enabled:true,max_positions:2,max_lots:0.1,max_risk_percent:12};
assert.equal(validatePortfolio(config).max_risk_percent,12);
for(const invalid of [
 {},{...config,max_positions:0},{...config,max_positions:1.5},
 {...config,max_lots:0},{...config,max_lots:NaN},
 {...config,max_risk_percent:0},{...config,max_risk_percent:101},
 {...config,enabled:'true'},{...config,extra:1},
]){
 assert.throws(()=>validatePortfolio(invalid),'invalid portfolio data must fail closed');
}
const terminal=fs.readFileSync(path.join(__dirname,'../src/terminal.tsx'),'utf8');
const tauri=fs.readFileSync(path.join(__dirname,'../../tauri/src/bridge.rs'),'utf8');
const ipc=fs.readFileSync(path.join(__dirname,'../src/bridge.ts'),'utf8');
const operations=fs.readFileSync(path.join(__dirname,'../src/micro-capital-operations.tsx'),'utf8');
for(const snippet of [
 "const ticket=scopedGeneration.current.beginRead('portfolio')",
 "scopedGeneration.current.isCurrent(ticket)",
 "savePortfolio(validatePortfolio(input))",
 "MicroCapitalOperations status={!preview&&feed.broker?status:null}",
 "armed={status?.requested_run===true}",
 "if(armed)return;",
])assert(terminal.includes(snippet),'missing guarded Core micro control: '+snippet);
assert(tauri.includes('fn update_portfolio(settings: Value)'));
assert(tauri.includes('"/v1/execution/portfolio"'));
assert(tauri.includes('update_risk, update_portfolio'));
assert(ipc.includes("invoke('update_portfolio',{settings})"));
assert(operations.includes('smallAccountPreview(status,risk,Number(scenario))'));
assert(operations.includes('onSavePortfolio(next)'));
assert(operations.includes('لا تغيّر الوسيط'));
assert(operations.includes('Micro capital and portfolio control'));
console.log('PASS: $50/$100 micro REAL scenario, broker leverage 1:500 facts, risk budgeting, unsafe/stale revocation, portfolio validation and native IPC wiring');
