const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),ts=require('typescript');
const dir=path.resolve(__dirname,'../src');
const content=file=>fs.readFileSync(path.join(dir,file),'utf8');
const transpile=(file,jsx=false)=>ts.transpileModule(content(file),{
 fileName:file,compilerOptions:{module:ts.ModuleKind.CommonJS,
 target:ts.ScriptTarget.ES2022,jsx:jsx?ts.JsxEmit.ReactJSX:undefined}
}).outputText;
const helper={};vm.runInNewContext(transpile('mode-evidence.ts'),{exports:helper});
const {fourModeObservation}=helper;
const NOW=Date.now();
const telemetry={connected:true,demo:true,account_trade_mode:0,login:17,
 server:'Demo',symbol:'XAUUSD',currency:'USD'};
const status={gateway_fresh:true,telemetry};
const modes=['sniper','scalping','swing','expert'];
const make=()=>({
 schema:'p1h-four-mode-evidence-v1',state:'observed',
 read_only:true,automatic_promotion:false,broker_execution_permission:false,
 verified_demo_account:true,independent_oos:false,observed_ms:NOW,
 account:{login:17,server:'Demo',symbol:'XAUUSD',currency:'USD',mode:0},
 context:JSON.stringify(['Demo',17,'XAUUSD','USD']),
 cross_mode_position_collisions:0,malformed_broker_samples:0,
 modes:modes.map(mode=>({mode,status:'descriptive_only',
   reason:'descriptive_evidence_only',closed_costed_samples:30,
   invalid_samples:0,duplicate_positions:0,minimum_samples:30,
   sample_cap_per_mode:240,distinct_strategy_sources:1,
   win_rate_after_costs:0.60,net_after_costs:24.5,
   source:'reconciled_closed_mt5_demo_coordinator_samples',
   online_learning_could_have_seen_these_results:true,independent_oos:false,
   promotion_authorized:false}))
});
const clone=v=>JSON.parse(JSON.stringify(v));
const truth=fourModeObservation(status,make(),NOW);
assert.equal(truth.available,true);
assert.equal(truth.rows.length,4);
assert.equal(truth.rows[0].samples,30);
assert.equal(truth.rows[2].winRate,0.60);
assert.equal(truth.rows[2].net,24.5);
function reject(label,change){
 const s=clone(status),p=make();change(s,p);
 const value=fourModeObservation(s,p,NOW);
 assert.equal(value.available,false,label);
 assert.equal(value.rows.length,0,label+' no prior account rows');
}
reject('stale gateway',s=>{s.gateway_fresh=false});
reject('broker disconnected',s=>{s.telemetry.connected=false});
reject('REAL cannot use Demo wins',s=>{s.telemetry.demo=false;s.telemetry.account_trade_mode=2});
reject('account switch',s=>{s.telemetry.login=18});
reject('symbol switch',s=>{s.telemetry.symbol='EURUSD'});
reject('server switch',s=>{s.telemetry.server='Other'});
reject('currency switch',s=>{s.telemetry.currency='EUR'});
reject('unknown broker trade mode',s=>{s.telemetry.account_trade_mode=null});
reject('report too old',(_,p)=>{p.observed_ms=NOW-20_000});
reject('clock from future',(_,p)=>{p.observed_ms=NOW+30_000});
reject('not read-only',(_,p)=>{p.read_only=false});
reject('promotion claim',(_,p)=>{p.automatic_promotion=true});
reject('execution claim',(_,p)=>{p.broker_execution_permission=true});
reject('false independent OOS',(_,p)=>{p.independent_oos=true});
reject('wrong context',(_,p)=>{p.context='[]'});
reject('wrong account mode',(_,p)=>{p.account.mode=2});
reject('omitted a trading mode',(_,p)=>{p.modes.pop()});
reject('duplicated a mode',(_,p)=>{p.modes[1].mode='sniper'});
reject('fake paper source',(_,p)=>{p.modes[0].source='paper'});
reject('sample count too small for descriptive',(_,p)=>{p.modes[0].closed_costed_samples=4});
reject('invalid row classified descriptive',(_,p)=>{p.modes[0].invalid_samples=2});
reject('cross-mode collision but descriptive',(_,p)=>{p.cross_mode_position_collisions=1});
reject('malformed account records but descriptive',(_,p)=>{p.malformed_broker_samples=1});
reject('unknown success percent',(_,p)=>{p.modes[0].win_rate_after_costs=1.20});
reject('win rate on insufficient',(_,p)=>{p.modes[0].status='insufficient_or_contaminated'});
const poor=make();
poor.modes[0].status='insufficient_or_contaminated';
poor.modes[0].reason='insufficient_closed_demo_deals';
poor.modes[0].closed_costed_samples=12;
poor.modes[0].win_rate_after_costs=null;
poor.modes[0].net_after_costs=1.4;
const safe=fourModeObservation(status,poor,NOW);
assert.equal(safe.available,true);
assert.equal(safe.rows[0].winRate,null);
const jsx=(type,props)=>({type,props:props||{}});
const component={};vm.runInNewContext(transpile('mode-evidence-operations.tsx',true),{
 exports:component,require:id=>{
  if(id==='react/jsx-runtime')return {jsx,jsxs:jsx};
  if(id==='./mode-evidence')return helper;
  throw Error('Unexpected dependency '+id);
 },Date,Number,String,JSON
});
function nodes(value,arr=[]){
 if(Array.isArray(value)){value.forEach(x=>nodes(x,arr));return arr;}
 if(!value||typeof value!=='object')return arr;
 arr.push(value);nodes(value.props?.children,arr);return arr;
}
let refreshed=0;
const panel=component.ModeEvidenceOperations({status,report:poor,disabled:false,onRefresh:()=>refreshed++});
const items=nodes(panel);
const refresh=items.find(n=>n.type==='button'&&n.props.children==='تحديث أدلة الأوضاع من Rust Core');
assert(refresh&&!refresh.props.disabled);
refresh.props.onClick();assert.equal(refreshed,1);
assert(items.filter(x=>x.type==='tr').length>=5,'four actual mode rows');
const stale=component.ModeEvidenceOperations({status:{...status,gateway_fresh:false},report:poor,disabled:false,onRefresh:()=>{}});
assert.equal(nodes(stale).filter(x=>x.type==='tr').length,0,'stale account never paints mode scores');
const core=fs.readFileSync(path.resolve(dir,'../../../crates/runtime/src/execution.rs'),'utf8');
const rustEvidence=fs.readFileSync(path.resolve(dir,'../../../crates/runtime/src/mode_evidence.rs'),'utf8');
const bridge=fs.readFileSync(path.resolve(dir,'../../tauri/src/bridge.rs'),'utf8');
const ui=content('terminal.tsx');
assert(core.includes('crate::mode_evidence::report(db,t,now)'));
assert(bridge.includes('"/v1/execution/mode-evidence"'));
assert(ui.includes('scopedGeneration.current.beginRead(\'modeEvidence\')'));
assert(ui.includes('<ModeEvidenceOperations status={!preview&&feed.broker?status:null}'));
assert(rustEvidence.includes('cross_mode_position_collisions'));
assert(rustEvidence.includes('malformed_broker_samples'));
assert(rustEvidence.includes('online_learning_could_have_seen_these_results'));
assert(!rustEvidence.includes('order_send('));
console.log('PASS: P1-H four-mode evidence with 25+ stale/REAL/fake/mixed-source negative contracts, scoped Windows UI refresh and no broker write');
