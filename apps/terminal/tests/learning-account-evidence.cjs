const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require('typescript'),base=path.resolve(__dirname,'../src');
const load=file=>{
 const r=ts.transpileModule(fs.readFileSync(path.join(base,file),'utf8'),{
 fileName:file,compilerOptions:{module:ts.ModuleKind.CommonJS,
 target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX},reportDiagnostics:true});
 assert.equal(r.diagnostics.filter(x=>x.category===ts.DiagnosticCategory.Error).length,0,file);
 return r.outputText;
};
const mod={};vm.runInNewContext(load('learning-account-evidence.ts'),{exports:mod,Number,Object,JSON});
const tsNow=1780000000000;
const account={connected:true,demo:true,account_trade_mode:0,login:17,
 server:'DEMO-X',symbol:'XAUUSD',currency:'USD'};
const status={gateway_fresh:true,telemetry:account};
const key=JSON.stringify([account.server,account.login,account.symbol,account.currency,0,true]);
const data={schema:'learning-account-evidence-v1',state:'observed',read_only:true,
 account_verified:true,observed_ms:tsNow,context:key,
 account:{login:17,server:'DEMO-X',symbol:'XAUUSD',currency:'USD',mode:0,demo:true},
 snapshots:5,pending_labels:2,matured_schema_eligible_labels:2,
 matured_incompatible_schema_labels:1,last_bar_time:10000,
 training_enabled:false,prediction_enabled:false,broker_win_rate:null,
 broker_net_pnl:null,automatic_promotion:false};
const show=(s=status,d=data,n=tsNow)=>mod.learningAccountView(s,d,n);
const v=show();
assert.equal(v.ready,true);assert.equal(v.mode,'DEMO');
assert.equal(v.snapshots,5);assert.equal(v.eligible,2);assert.equal(v.pending,2);
assert.equal(v.incompatible,1);assert.equal(v.trainingEnabled,false);
assert.equal(v.predictionEnabled,false);
assert.equal(show(status,{...data,snapshots:0,pending_labels:0,
 matured_schema_eligible_labels:0,matured_incompatible_schema_labels:0,last_bar_time:null}).ready,true);
for(const [label,s,d,n] of [
 ['wrong login',{...status,telemetry:{...account,login:18}},data,tsNow],
 ['same login other symbol',{...status,telemetry:{...account,symbol:'EURUSD'}},data,tsNow],
 ['REAL substitution',{...status,telemetry:{...account,demo:false,account_trade_mode:2}},data,tsNow],
 ['mode contradiction',{...status,telemetry:{...account,account_trade_mode:2}},data,tsNow],
 ['stale Core',status,data,tsNow+20000],
 ['gateway stale',{...status,gateway_fresh:false},data,tsNow],
 ['account mismatch',status,{...data,account:{...data.account,login:19}},tsNow],
 ['scope mismatch',status,{...data,context:'legacy-four-field-scope'},tsNow],
 ['wrong aggregation',status,{...data,snapshots:6},tsNow],
 ['sample-count overflow',status,{...data,pending_labels:Number.MAX_SAFE_INTEGER+1},tsNow],
 ['fake PnL',status,{...data,broker_net_pnl:250.2},tsNow],
 ['fake broker winning percent',status,{...data,broker_win_rate:0.99},tsNow],
 ['promoted challenger',status,{...data,automatic_promotion:true},tsNow],
 ['claimed training enabled',status,{...data,training_enabled:true},tsNow],
 ['future result',status,{...data,observed_ms:tsNow+3000},tsNow],
 ['unavailable backend',status,{...data,state:'unavailable',account_verified:false},tsNow],
 ]){
  const x=show(s,d,n);assert.equal(x.ready,false,label);
  assert.equal(x.snapshots,null,label);assert.equal(x.eligible,null,label);
}
const jsx=(type,props)=>({type,props:props||{}});
const component={};vm.runInNewContext(load('learning-account-operations.tsx'),{
 exports:component,require:id=>{
  if(id==='react')return{useState:init=>[init,()=>{}],useEffect:()=>{}};
  if(id==='./learning-account-evidence')return mod;
  if(id==='./bridge')return {readPanel:()=>Promise.reject(Error('offline'))};
  if(id==='react/jsx-runtime')return{jsx,jsxs:jsx};
  throw Error('Unexpected module '+id);
 },Number,Math,Object,JSON,Date,console});
const node=component.LearningAccountOperations({status,scope:'ctx-demo',preview:false});
assert.equal(node.props['aria-label'],'Verified account research learning evidence');
const view=fs.readFileSync(path.join(base,'learning-account-operations.tsx'),'utf8');
const terminal=fs.readFileSync(path.join(base,'terminal.tsx'),'utf8');
const tauri=fs.readFileSync(path.join(base,'../../tauri/src/bridge.rs'),'utf8');
const rust=fs.readFileSync(path.join(base,'../../../crates/runtime/src/lib.rs'),'utf8');
for(const x of ["readPanel('/v1/learning/account-evidence')","learningAccountView(status,slot.report,Date.now())",
 "عينات ناضجة متوافقة","ليست صفقات مغلقة"])assert(view.includes(x),x);
assert(terminal.includes("<LearningAccountOperations key={viewScope}"));
assert(terminal.includes("السجل البحثي العام — لا يمثل الحساب الحالي وحده"));
assert(tauri.includes('"/v1/learning/account-evidence"'));
assert(rust.includes('("GET","/v1/learning/account-evidence")'));
console.log('PASS G5-A: real current account/symbol/mode research M1 evidence, exact schema counts, no invented broker performance, honest global registry separation, native JSX');
