const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const path=require('node:path');
const ts=require('typescript');
const root=path.resolve(__dirname,'..');
const file=p=>fs.readFileSync(path.join(root,p),'utf8');
const source=file('src/partial-exit-preview.tsx');
const compiled=ts.transpileModule(source,{
 fileName:'partial-exit-preview.tsx',
 compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX},
 reportDiagnostics:true
});
assert.equal((compiled.diagnostics||[]).filter(x=>x.category===ts.DiagnosticCategory.Error).length,0);
const exports={};
const el=(type,props)=>({type,props:props||{}});
vm.runInNewContext(compiled.outputText,{
 exports,require:id=>{
  if(id==='react/jsx-runtime')return {jsx:el,jsxs:el};
  throw Error('Unexpected dependency '+id);
 },Number,Object,Array,JSON,Math,String
});
const render=exports.PartialExitPreview;
function collect(n,all=[]){
 if(n===null||n===undefined)return all;
 if(Array.isArray(n)){n.forEach(x=>collect(x,all));return all}
 if(typeof n==='object'){all.push(n);collect(n.props?.children,all);return all}
 all.push(n);return all;
}
const shown=n=>collect(n).filter(v=>typeof v==='string'||typeof v==='number').join(' ');
const tel={connected:true,demo:true,account_trade_mode:0,login:17,server:'D',currency:'USD',symbol:'XAUUSDm'};
const status={gateway_fresh:true,telemetry:tel};
const context=JSON.stringify([tel.server,tel.login,tel.symbol,tel.currency]);
const view={
 account_matches:true,context,
 settings:{target:'demo',login:17,server:'D',currency:'USD'},
 partial_exit_preview:{
  schema:'partial-exit-volume-preview-v1',
  scope:'owned_positions_current_broker_account_symbol',
  context,target:'demo',status:'read_only_geometry',
  read_only:true,orders_enabled:false,volume_min:0.01,volume_step:0.01,volume_max:100,
  rows:[{position:77,position_volume:0.1,state:'geometry_valid',
    lots:{tp1:0.05,tp2:0.03,tp3:0.02}},
   {position:88,position_volume:0.02,state:'unavailable',
    reason:'too_small_for_three_broker_valid_exits',lots:null}]
 }
};
const rendered=render({status,source:view}),parts=collect(rendered);
assert(parts.some(x=>x.type==='table'),'Renders actual Core preview table');
assert(!parts.some(x=>x.type==='button'),'Read-only cannot send orders');
assert(shown(rendered).includes('0.05') && shown(rendered).includes('0.03'));
assert(shown(rendered).includes('أصغر من ثلاثة إغلاقات'));
assert(shown(rendered).includes('ليست خاصية TP جزئي جاهزة'));
const mismatched=render({status,source:{...view,context:'other-account'}});
assert(!collect(mismatched).some(x=>x.type==='table'),'Do not render cross-account volumes');
const stale=render({status:{...status,gateway_fresh:false},source:view});
assert(!collect(stale).some(x=>x.type==='table'),'Do not render stale account volumes');
const writable=render({status,source:{...view,partial_exit_preview:{...view.partial_exit_preview,orders_enabled:true}}});
assert(!collect(writable).some(x=>x.type==='table'),'A response asserting order permission must fail closed');
const impossible=render({status,source:{...view,partial_exit_preview:{
 ...view.partial_exit_preview,rows:[{position:77,position_volume:0.1,state:'geometry_valid',
 lots:{tp1:0.05,tp2:0.04,tp3:0.04}}]
}}});
assert(!shown(impossible).includes('تقسيم حجم صالح حسابياً فقط'));
const core=file('../../crates/runtime/src/execution.rs');
const domain=file('../../crates/domain/src/partial_exit.rs');
const terminal=file('src/terminal.tsx');
assert(core.includes('value["partial_exit_preview"]=if status.gateway_fresh && self.account_matches(status)'));
assert(domain.includes('pub fn three_target_lots('));
assert(domain.includes('pub fn preview('));
assert(terminal.includes('<PartialExitPreview status={!preview&&feed.broker?status:null} source={portfolioReport}/>'));
console.log('PASS: TP preview fail-closed account binding, source geometry, small lot warnings, and zero order controls');
