const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require('typescript'),assert=require('node:assert/strict');
const src=fs.readFileSync(path.join(__dirname,'../src/micro-capital.ts'),'utf8');
const js=ts.transpileModule(src,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});
const attested={};const attestationSource=fs.readFileSync(path.join(__dirname,'../src/micro-readiness.ts'),'utf8');
const attestationJs=ts.transpileModule(attestationSource,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});
vm.runInNewContext(attestationJs.outputText,{exports:attested});
const loaded={};vm.runInNewContext(js.outputText,{exports:loaded,require:id=>{
 if(id==='./micro-readiness')return attested;
 throw Error('Unknown dependency '+id);
}});
const {smallAccountPreview,validatePortfolio}=loaded;
const t={connected:true,demo:false,account_trade_mode:2,login:871250,server:'Broker-REAL',equity:50,balance:50,currency:'USD',leverage:500,
 margin_free:45,minimum_lot_margin:12,volume_min:0.01};
const risk={risk_percent:0.25,micro_real_temporary:true,micro_real_cap_percent:10,micro_real_recovery_equity:500};
const observed=Date.now();
const monitor=(telemetry=t,state='candidate_only',cap=10,enabled=true)=>({
 state:'ready',enabled:true,reason:'قاطع يومي مفعل',observed_ms:observed,
 account:{login:telemetry.login,server:telemetry.server,currency:telemetry.currency,mode:telemetry.account_trade_mode},
 micro_real:{schema:'p1e-micro-recovery-v1',read_only:true,broker_execution_authorized:false,
  observed_ms:observed,
  account:{login:telemetry.login,server:telemetry.server,currency:telemetry.currency,mode:telemetry.account_trade_mode},
  state,candidate_only:state==='candidate_only',configured:enabled,recovered_persisted:state==='recovered_latched',
  current_milestone_reached:state==='milestone_reached',cap_percent:cap,recovery_equity:500}
});
const preview=(telemetry=t,policy=risk,scenario=500,audit=monitor(telemetry))=>
 smallAccountPreview({gateway_fresh:true,telemetry},policy,scenario,audit,observed);
const view=preview(t,risk,100);
assert.equal(view.available,true);
assert.equal(view.accountType,'REAL');
assert.equal(view.equity,50);
assert.equal(view.login,871250);
assert.equal(view.server,'Broker-REAL');
assert.equal(view.leverage,500,'real MT5 leverage must never be replaced by scenario');
assert.equal(view.minimumLot,0.01);
assert.equal(view.minimumLotMargin,12);
assert.equal(view.scenarioMargin,60,'scenario 1:100 is indicative only');
assert.equal(view.marginCovered,true);
assert.equal(view.microCandidate,true);
assert.equal(view.budgetPercent,10);
assert.equal(view.budget,5);
const lev=preview(t,risk,500);
assert.equal(lev.scenarioMargin,12,'same leverage preserves indicative broker minimum margin');
const omitted=preview(t,{risk_percent:0.25,micro_real_temporary:true},500);
assert.equal(omitted.microCandidate,true,'serde omits default recovery milestone of 500');
assert.equal(omitted.budgetPercent,10,'serde omits default Micro cap of 10%');
assert.equal(omitted.budget,5,'50 USD micro default remains 5 USD estimated risk');
const unconfigured=preview(t,null,500);
assert.equal(unconfigured.budget,null,'missing Core risk config is not interpreted as default');
const fifty=preview(t,{...risk,micro_real_cap_percent:20},500,monitor(t,'candidate_only',20));
assert.equal(fifty.budget,10,'20% micro is a high-risk estimate, never an order authorization');
const a100=preview({...t,equity:100},risk,500);
assert.equal(a100.budget,10,'user can choose account 100 without a configured minimum deposit');
const recovered=preview({...t,equity:501},risk,500,monitor({...t,equity:501},'milestone_reached'));
assert.equal(recovered.microCandidate,false);
assert.equal(recovered.budget,1.2525,'falls back to normal per trade policy after threshold');
const notEnabled=preview(t,{...risk,micro_real_temporary:false},500,monitor(t,'disabled',10,false));
assert.equal(notEnabled.budget,0.125);
const demo=preview({...t,account_trade_mode:0,demo:true},risk,500,monitor({...t,account_trade_mode:0,demo:true},'unverified_or_demo'));
assert.equal(demo.microCandidate,false,'REAL opt-in can never bleed into Demo');
assert.equal(demo.budget,0.125);
for(const status of [
 null,
 {gateway_fresh:false,telemetry:t},
 {gateway_fresh:true,telemetry:{...t,connected:false}},
 {gateway_fresh:true,telemetry:{...t,demo:true,account_trade_mode:2}},
 {gateway_fresh:true,telemetry:{...t,equity:0}},
 {gateway_fresh:true,telemetry:{...t,equity:NaN}},
 {gateway_fresh:true,telemetry:{...t,login:0}},
 {gateway_fresh:true,telemetry:{...t,server:''}},
]){
 const value=smallAccountPreview(status,risk,500);
 assert.equal(value.available,false);
 assert.equal(value.budget,null,'unverified state must not synthesize a positive trade budget');
}
const noMargin=preview({...t,minimum_lot_margin:null,leverage:null},risk,500);
assert.equal(noMargin.scenarioMargin,null);assert.equal(noMargin.marginCovered,null);
const insufficient=preview({...t,margin_free:5},risk,500);
assert.equal(insufficient.marginCovered,false);
// No browser-local fallback is allowed when Core has latched recovery.
const latched=preview(t,risk,500,monitor(t,'recovered_latched'));
assert.equal(latched.microCandidate,false);
assert.equal(latched.budget,0.125);
const noAttestation=smallAccountPreview({gateway_fresh:true,telemetry:t},risk,500);
assert.equal(noAttestation.budget,null,'missing durable Core recovery evidence revokes REAL Micro estimate');
const mismatched=preview(t,risk,500,monitor({...t,login:918001}));
assert.equal(mismatched.budget,null,'other broker login cannot certify Micro budget');
const stale=preview(t,risk,500,{...monitor(t),observed_ms:observed-30_000});
assert.equal(stale.budget,null,'stale risk monitor cannot certify Micro budget');
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
assert(operations.includes('smallAccountPreview(status,risk,Number(scenario),monitor)'));
assert(operations.includes('onSavePortfolio(next)'));
assert(operations.includes('لا تغيّر الوسيط'));
assert(operations.includes('Micro capital and portfolio control'));
console.log('PASS: $50/$100 micro REAL scenario, broker leverage 1:500 facts, risk budgeting, unsafe/stale revocation, portfolio validation and native IPC wiring');
