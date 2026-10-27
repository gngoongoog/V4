const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),ts=require('typescript');
const dir=path.resolve(__dirname,'../src');
const load=file=>fs.readFileSync(path.join(dir,file),'utf8');
const exportsObj={},jsx=(type,props)=>({type,props:props||{}});
vm.runInNewContext(ts.transpileModule(load('sniper-tick-evidence.tsx'),{
 fileName:'sniper-tick-evidence.tsx',compilerOptions:{
 module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}
}).outputText,{exports:exportsObj,require:id=>{
 if(id==='react/jsx-runtime')return{jsx,jsxs:jsx};
 throw Error('Unexpected dependency '+id);
},JSON,Number,String,Date,Object});
const {sniperTickEvidence,SniperTickEvidence}=exportsObj;
const now=1_000_000;
const telemetry={connected:true,demo:true,account_trade_mode:0,login:17,
 server:'Demo',symbol:'XAUUSD',currency:'USD'};
const status={gateway_fresh:true,telemetry};
const report={
 schema:'p1k2-sniper-accepted-pulse-audit-v1',status:'observed',
 context:JSON.stringify(['Demo',17,'XAUUSD','USD']),
 account:{login:17,server:'Demo',symbol:'XAUUSD',currency:'USD'},
 read_only:true,no_broker_orders:true,broker_tick_delivery_complete:false,
 estimated_missing_market_ticks:null,observed_market_tick_count:null,
 last_accepted_ms:now-50,accepted_pulses:100,accepted_native_pulses:60,
 accepted_poll_fallback_pulses:40,accepted_unknown_origin_pulses:0,
 native_poll_source_switches:2,observed_quote_gaps_gt_1500ms:3,
 largest_observed_quote_gap_ms:2400,same_broker_millisecond_pulses:4,
 reported_native_mailbox_overflow_since_baseline:1
};
const clone=x=>JSON.parse(JSON.stringify(x));
assert.equal(sniperTickEvidence(status,report,false,now).available,true);
function deny(reason,modify){
 const s=clone(status),r=clone(report);
 modify(s,r);
 const result=sniperTickEvidence(s,r,false,now);
 assert.equal(result.available,false,reason);
 assert.deepEqual(Object.keys(result.data),[],reason+' must clear scores');
}
deny('REAL',s=>{s.telemetry.demo=false;s.telemetry.account_trade_mode=2});
deny('disconnected',s=>{s.telemetry.connected=false});
deny('stale',s=>{s.gateway_fresh=false});
deny('login',s=>{s.telemetry.login=18});
deny('server',s=>{s.telemetry.server='Different'});
deny('symbol',s=>{s.telemetry.symbol='EURUSD'});
deny('context',(_,r)=>{r.context='[]'});
deny('old response',(_,r)=>{r.last_accepted_ms=now-16000});
deny('future accepted quote',(_,r)=>{r.last_accepted_ms=now+5000});
deny('missing Native + Poll count',(_,r)=>{delete r.accepted_native_pulses});
deny('market data completeness claim',(_,r)=>{r.broker_tick_delivery_complete=true});
deny('fabricated missing tick count',(_,r)=>{r.estimated_missing_market_ticks=99});
deny('pretend a backtest result is Tick audit',(_,r)=>{r.schema='paper'});
let clicked=0;
const render=SniperTickEvidence({status,report,preview:false,onRefresh:()=>clicked++});
function walk(node,arr=[]){
 if(Array.isArray(node)){node.forEach(v=>walk(v,arr));return arr}
 if(!node||typeof node!=='object')return arr;
 arr.push(node);walk(node.props?.children,arr);return arr;
}
const buttons=walk(render).filter(x=>x.type==='button');
assert.equal(buttons.length,1);
buttons[0].props.onClick();assert.equal(clicked,1);
const tauri=fs.readFileSync(path.resolve(dir,'../../tauri/src/bridge.rs'),'utf8');
const core=fs.readFileSync(path.resolve(dir,'../../../crates/runtime/src/execution.rs'),'utf8');
const ui=load('terminal.tsx');
assert(tauri.includes('"/v1/execution/sniper-tick-audit"'));
assert(core.includes('self.sniper_tick_audit.observe('));
assert(core.includes('self.sniper_tick_audit.report(scoped,now)'));
assert(ui.includes("scopedGeneration.current.beginRead('sniperTickAudit')"));
assert(ui.includes('sniperAuditSlot?.scope===viewScope?sniperAuditSlot.report:null'));
assert(ui.includes('<SniperTickEvidence status={!preview&&feed.broker?status:null}'));
console.log('PASS: P1-K2 Demo-only account-scoped Native/Poll Tick audit, stale data concealment and actual Windows refresh');
