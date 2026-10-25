const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require('typescript'),root=path.resolve(__dirname,'../src');
const load=(name)=>ts.transpileModule(fs.readFileSync(path.join(root,name),'utf8'),{
 compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX},
 fileName:name,reportDiagnostics:true}).outputText;
const jsx=(type,props)=>({type,props:props||{}});
const quality={};vm.runInNewContext(load('execution-quality.ts'),{exports:quality,Number,JSON,Object,Set});
const actual={};vm.runInNewContext(load('dashboard-evidence.ts'),{
 exports:actual,require:id=>{if(id==='./execution-quality')return quality;throw Error('Unexpected import '+id)},
 Number,JSON,Object,Set});
const cards={};vm.runInNewContext(load('dashboard-operations.tsx'),{
 exports:cards,require:id=>{
  if(id==='./dashboard-evidence')return actual;
  if(id==='react/jsx-runtime')return {jsx,jsxs:jsx};
  throw Error('Unexpected import '+id);
 },Number,JSON,Object,Set});
const now=Date.now(),t={connected:true,demo:true,account_trade_mode:0,
 login:10,server:'DEMO',currency:'USD',symbol:'XAUUSD'};
const status={gateway_fresh:true,telemetry:t},scope='test-bound-account';
const bound={account_matches:true,context:JSON.stringify([t.server,t.login,t.symbol,t.currency])};
const trades={total:2,offset:0,limit:50,rows:[
 {position_id:901,symbol:'XAUUSD',side:'BUY',status:'CLOSED',
  lots_opened:0.1,broker_net_booked:12.75,booked_cost_evidence:'BROKER_COMPONENTS_VERIFIED'},
 {position_id:902,symbol:'XAUUSD',side:'SELL',status:'EXIT SEEN / CLOSE NOT VERIFIED',
  lots_opened:0.2,broker_net_booked:42,booked_cost_evidence:'MISSING_OR_INVALID_BROKER_COST'}
]};
const q={schema:'p1f-execution-quality-v1',state:'observed',read_only:true,
 account_verified:true,observed_ms:now,context:bound.context,
 account:{server:t.server,login:t.login,symbol:t.symbol,currency:t.currency,mode:0},
 sample_size:2,rejected_count:1,unknown_count:0,unresolved_count:1,
 rejection_categories:{market_closed:1},
 queue_to_send:{samples:1,p95_ms:7},send:{samples:1,p95_ms:11},
 total:{samples:1,p95_ms:18},rows:[]};
const read={scope,readMs:now,execution:bound,trades,quality:q};
const view=actual.dashboardView(status,read,scope,now);
assert.equal(view.ready,true);
assert.equal(view.trades.length,2);
assert.equal(view.trades[0].net,12.75);
assert.equal(view.trades[1].net,null,'missing costs do not become nominal profits');
assert.equal(view.quality.sendP95,11);
for(const [name,s,data,sc,at] of [
 ['changed login',{...status,telemetry:{...t,login:11}},read,scope,now],
 ['DEMO to REAL',{...status,telemetry:{...t,demo:false,account_trade_mode:2}},read,scope,now],
 ['stale feed',{...status,gateway_fresh:false},read,scope,now],
 ['previous page scope',status,read,'another-scope',now],
 ['expired report',status,read,scope,now+21000],
 ['unbound execution',status,{...read,execution:{...bound,account_matches:false}},scope,now],
 ['wrong execution context',status,{...read,execution:{...bound,context:'other-account'}},scope,now],
 ['different symbol',status,{...read,trades:{...trades,rows:[{...trades.rows[0],symbol:'EURUSD'}]}},scope,now],
 ['fabricated closed result',status,{...read,trades:{...trades,rows:[{...trades.rows[0],status:'WIN'}]}},scope,now],
 ['invalid pagination',status,{...read,trades:{...trades,offset:-1}},scope,now]
 ]){
 const res=actual.dashboardView(s,data,sc,at);
 assert.equal(res.ready,false,name);
 assert.equal(res.total,null,name);
 assert.equal(res.quality,null,name);
 assert.equal(res.trades.length,0,name);
}
const qualityStale=actual.dashboardView(status,{...read,quality:{...q,observed_ms:now-20_000}},scope,now);
assert.equal(qualityStale.ready,true,'journal is useful even without a valid quality report');
assert.equal(qualityStale.quality,null);
const walk=(v,out=[])=>{
 if(!v||typeof v!=='object')return out;
 if(Array.isArray(v)){v.forEach(i=>walk(i,out));return out;}
 out.push(v);walk(v.props?.children,out);return out;
};
const feed=cards.DashboardTradeFeed({status,read,scope,now,preview:false,onJournal:()=>{}});
const nodes=walk(feed);
assert(nodes.some(x=>x.type==='strong'&&String(x.props.children).includes('12.75')));
assert(nodes.some(x=>x.type==='strong'&&x.props.children==='صافي غير موثّق'));
const masked=cards.DashboardTradeFeed({status:{...status,telemetry:{...t,login:11}},
 read,scope,now,preview:false,onJournal:()=>{}});
assert(!walk(masked).some(x=>x.type==='strong'&&String(x.props.children).includes('12.75')));
const qualityCard=cards.DashboardExecutionQuality({status,read,scope,now,preview:false,onExecution:()=>{}});
assert(walk(qualityCard).some(x=>x.type==='small'&&x.props.children==='11 ms'));
const preview=cards.DashboardExecutionQuality({status,read,scope,now,preview:true,onExecution:()=>{}});
assert(walk(preview).some(x=>x.type==='p'&&String(x.props.children).includes('DEMONSTRATION ONLY')));
const main=fs.readFileSync(path.join(root,'terminal.tsx'),'utf8');
for(const required of [
 "readPanel('/v1/execution/trades')","readPanel('/v1/execution/quality')",
 "readPanel('/v1/execution')","scopedGeneration.current.beginRead('dashboardBroker')",
 "<DashboardTradeFeed","<DashboardExecutionQuality","scope={contextKey}"])
 assert(main.includes(required),'real dashboard binding missing '+required);
assert(!main.includes('الصفقات التاريخية تُقرأ من سجل Core في صفحة السجلات.'),
 'remove the permanent placeholder from LIVE dashboard');
console.log('PASS G3-A: real account-scoped trade/quality cards, no invented costs, cross-account/stale protection, functioning component paths');
