const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const ts=require('typescript');
const path=require('node:path');
const src=fs.readFileSync(path.join(__dirname,'../src/brain-evidence.ts'),'utf8');
const js=ts.transpileModule(src,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020}});
const exports={};vm.runInNewContext(js.outputText,{exports});
const fn=exports.brokerEvidence;
const status={gateway_fresh:true,telemetry:{connected:true,demo:true,account_trade_mode:0,login:17,server:'Demo',symbol:'XAUUSD',currency:'USD'}};
const quality={source:'Closed MT5 DEMO deal records; net=gross+commission+swap+fee',
 policy:'read_only',paper_and_forward_excluded:true,no_auto_promotion:true,
 status:'monitor_only',broker_samples:32,
 all:{samples:32,wins_after_costs:19,win_rate:19/32,net_booked:24.50}};
const evidence={source:'closed_costed_mt5_demo_deals_only',
 scope:'current_demo_account_symbol_mode_brain',no_auto_promotion:true,
 broker_execution_permission:false,status:'monitor_only',
 sample_count:32,minimum_samples:30,win_rate:19/32,net_booked:24.5};
const brains=Array.from({length:17},(_,i)=>({config:{id:i===0?'system':'brain-'+i},
 integrity:{role:i===0?'selected_mode_coordinator':'advisor_or_research',promotion_enabled:false,
 broker_evidence:i===0?evidence:{status:'insufficient_evidence'}},
 quality:i===0?quality:null}));
const base={context:JSON.stringify(['Demo',17,'XAUUSD','USD']),account_matches:true,
 settings:{target:'demo'},trading_mode:'sniper',
 brain_integrity:{schema:'p1-brain-integrity-v1',inventory_complete:true,
 brains_accounted_for:17,demo_account_verified:true,account_trade_mode:0,
 read_only:true,changes_execution:false,automatic_promotion:false,trading_mode:'sniper'},
 brains};
const clone=x=>JSON.parse(JSON.stringify(x));
function reject(label,mutate){const r=clone(base),s=clone(status);mutate(r,s);assert.equal(fn(r,s),null,label)}
assert.equal(fn(base,status).samples,32);
assert.equal(fn(base,status).losses,13);
assert(Math.abs(fn(base,status).winRate-59.375)<1e-10);
reject('no heartbeat',(_,s)=>{s.gateway_fresh=false});
reject('lost connection',(_,s)=>{s.telemetry.connected=false});
reject('REAL must not inherit Demo rate',(_,s)=>{s.telemetry.demo=false;s.telemetry.account_trade_mode=2});
reject('changed broker login',(_,s)=>{s.telemetry.login=18});
reject('changed broker server',(_,s)=>{s.telemetry.server='Other'});
reject('changed symbol',(_,s)=>{s.telemetry.symbol='EURUSD'});
reject('changed currency',(_,s)=>{s.telemetry.currency='EUR'});
reject('unbound data',r=>{r.account_matches=false});
reject('changed target',r=>{r.settings.target='live'});
reject('unverified Demo',r=>{r.brain_integrity.demo_account_verified=false});
reject('missing Core integrity',r=>{delete r.brain_integrity});
reject('cross mode report',r=>{r.trading_mode='expert'});
reject('missing inventory',r=>{r.brains.pop()});
reject('fake paper outcomes',r=>{r.brains[0].quality.source='paper'});
reject('fake broker evidence',r=>{r.brains[0].integrity.broker_evidence.source='paper'});
reject('under 30 trades',r=>{
 const e=r.brains[0].integrity.broker_evidence,q=r.brains[0].quality;
 e.sample_count=25;e.win_rate=14/25;e.net_booked=42;
 q.broker_samples=25;q.all={samples:25,wins_after_costs:14,win_rate:14/25,net_booked:42};
 e.status='insufficient_evidence';q.status='insufficient_evidence';
});
reject('mismatched count',r=>{r.brains[0].quality.broker_samples=42});
reject('mismatched win rate',r=>{r.brains[0].quality.all.wins_after_costs=20});
reject('mismatched net after costs',r=>{r.brains[0].quality.all.net_booked=1});
reject('mixed policy sample',r=>{
 r.brains[0].quality.status='mixed_policy_monitor_only';
 r.brains[0].integrity.broker_evidence.status='mixed_policy_monitor_only';
});
reject('automatic promotion',r=>{r.brain_integrity.automatic_promotion=true});
reject('bad role',r=>{r.brains[0].integrity.role='advisor_or_research'});
assert.equal(fn(base,null),null);
assert.equal(fn({},status),null);
console.log('PASS: P1-B dashboard Core-gated Demo KPI, 17-brain inventory, account/mode and 25 negative contracts');
