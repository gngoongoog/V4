const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const path=require('node:path');
const ts=require('typescript');
const root=path.resolve(__dirname,'..');
const read=(p)=>fs.readFileSync(path.join(root,p),'utf8');
const source=read('src/partial-exit-price-operations.tsx');
const compiled=ts.transpileModule(source,{fileName:'partial-exit-price-operations.tsx',
 compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,
  jsx:ts.JsxEmit.ReactJSX},reportDiagnostics:true});
assert.equal((compiled.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
const exp={},el=(type,props)=>({type,props:props||{}});
let selected='',inputs=['','',''],dirty=false,hook=0;
const useState=(init)=>{const value=hook++===0?selected:hook===2?inputs:dirty;
 return [value,()=>{}]};
vm.runInNewContext(compiled.outputText,{
 exports:exp,require:(id)=>{
  if(id==='react/jsx-runtime')return {jsx:el,jsxs:el};
  if(id==='react')return {useState};
  throw Error('Unexpected module: '+id);
 },Number,Object,Array,JSON,Math,String
});
function collect(n,out=[]){
 if(n==null)return out;
 if(Array.isArray(n)){for(const k of n)collect(k,out);return out}
 if(typeof n==='object'){out.push(n);collect(n.props?.children,out);return out}
 out.push(n);return out;
}
const render=(opts)=>{hook=0;return exp.PartialExitPriceOperations({...opts,onSave:async()=>true})};
const shown=(root)=>collect(root).filter(x=>typeof x==='string').join(' ');
const tel={connected:true,demo:true,account_trade_mode:0,margin_mode:2,
 login:17,server:'D',symbol:'XAUUSDm',currency:'USD',
 tick_size:0.01,bid:2000,ask:2000.2,
 positions:[{identifier:907,ticket:87,magic:410601,side:'BUY',
 symbol:'XAUUSDm',price_open:1999.5,volume:0.1,tp:0}]};
const status={gateway_fresh:true,requested_run:false,telemetry:tel};
const context=JSON.stringify([tel.server,tel.login,tel.symbol,tel.currency]);
const stages=[1,2,3].map((stage)=>({position_identifier:907,stage,
 plan_id:'tp-plan-'+String(stage).repeat(32),state:'prepared_only'}));
const view={account_matches:true,context,
 settings:{target:'demo',login:17,server:'D',currency:'USD'},
 partial_exit_stage_plans:{schema:'partial-exit-stage-plans-v1',
 context,target:'demo',read_only:true,orders_enabled:false,
 stage_execution_supported:false,rows:stages},
 partial_exit_price_proposals:{schema:'partial-exit-prices-v1',
 status:'demo_operator_price_proposals_only',
 context,target:'demo',read_only:true,orders_enabled:false,rows:[]}};
const opts={status,source:view,disabled:false};
let root=render(opts);
assert(shown(root).includes('DEMO فقط'));
assert(shown(root).includes('لا تُرسل أوامر'));
assert(!collect(root).some(x=>x.type==='button'&&!x.props.disabled),
 'Must not save until a position/three prices are selected');
selected='907';inputs=['2001','2002','2003'];dirty=true;
root=render(opts);
assert(collect(root).some(x=>x.type==='button'&&!x.props.disabled),
 'Exactly staged, broker-tick aligned, increasing prices can be saved');
assert(collect(root).filter(x=>x.type==='input').length===3);
inputs=['2001','2001','2003'];
assert(!collect(render(opts)).some(x=>x.type==='button'&&!x.props.disabled),
 'Decreasing/nonmonotone BUY prices must never be saved');
inputs=['2001.005','2002','2003'];
assert(!collect(render(opts)).some(x=>x.type==='button'&&!x.props.disabled),
 'Off-grid broker price must disable save');
inputs=['2001','2002','2003'];
const fake={...view,context:'other-account'};
assert(!collect(render({...opts,source:fake})).some(x=>x.type==='button'&&!x.props.disabled));
assert(!collect(render({...opts,status:{...status,requested_run:true}}))
 .some(x=>x.type==='button'&&!x.props.disabled));
assert(!collect(render({...opts,status:{...status,telemetry:{...tel,demo:false,account_trade_mode:2}}}))
 .some(x=>x.type==='button'&&!x.props.disabled));
const changed={...view,partial_exit_stage_plans:{...view.partial_exit_stage_plans,
 rows:stages.map(x=>({...x,state:'review_required'}))}};
assert(!collect(render({...opts,source:changed})).some(x=>x.type==='button'&&!x.props.disabled));
const stored={...view,partial_exit_price_proposals:{...view.partial_exit_price_proposals,
 rows:[{position_identifier:907,prices:[2001,2002,2003]}]}};
assert(!collect(render({...opts,source:stored})).some(x=>x.type==='button'&&!x.props.disabled));
assert(shown(render({...opts,source:stored})).includes('محفوظ للمركز'));
const bridge=read('src/bridge.ts');
assert(bridge.includes("invoke('save_partial_exit_prices'"));
const tauri=fs.readFileSync(path.join(root,'../tauri/src/bridge.rs'),'utf8');
assert(tauri.includes('fn save_partial_exit_prices('));
assert(tauri.includes('"/v1/execution/partial-exit-prices"'));
assert(tauri.includes('update_position_protection, save_partial_exit_prices,'));
const core=fs.readFileSync(path.join(root,'../../crates/runtime/src/execution.rs'),'utf8');
assert(core.includes('path=="/v1/execution/partial-exit-prices"'));
assert(core.includes('!status.requested_run && !self.pending(db)?'));
const prices=fs.readFileSync(path.join(root,'../../crates/runtime/src/partial_exit_prices.rs'),'utf8');
assert(!prices.includes('order_send('));
assert(prices.includes('orders_enabled":false'));
console.log('PASS: scoped and STOP-only DEMO TP price capture, stale/read-only/frozen controls');
