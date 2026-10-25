const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require('typescript');
const root=path.resolve(__dirname,'../src');
const transpile=(file)=>ts.transpileModule(fs.readFileSync(path.join(root,file),'utf8'),{
 fileName:file,compilerOptions:{target:ts.ScriptTarget.ES2022,
 module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX},reportDiagnostics:true});
const out={};const pure=transpile('journal-evidence.ts');
assert.equal(pure.diagnostics.filter(x=>x.category===ts.DiagnosticCategory.Error).length,0);
vm.runInNewContext(pure.outputText,{exports:out,Number,JSON,Object,Set});
const t={connected:true,demo:true,account_trade_mode:0,login:100,
 server:'MT5-Demo',symbol:'XAUUSD',currency:'USD'};
const status={gateway_fresh:true,telemetry:t};
const ctx=JSON.stringify([t.server,t.login,t.symbol,t.currency]);
const execution={account_matches:true,context:ctx,
 settings:{target:'demo',login:t.login,server:t.server,currency:t.currency}};
const trade={
 position_id:907,symbol:'XAUUSD',side:'BUY',status:'CLOSED',
 opened_utc:'2026-10-25 03:00:00 UTC',lots_opened:.1,lots_closed:.1,remaining_lots:null,
 broker_net_booked:14.3,broker_gross:15,commission:-.7,swap:0,fee:0,
 booked_cost_evidence:'BROKER_COMPONENTS_VERIFIED',
 origin:{opening_brain:'system',reason:'Broker opening command received'}
};
const source={context:ctx,total:73,offset:25,limit:25,rows:[
 trade,{...trade,position_id:908,booked_cost_evidence:'MISSING_OR_INVALID_BROKER_COST'}
]};
const view=out.journalEvidence(status,execution,source,25,25);
assert.equal(view.ready,true);
assert.equal(view.total,73);assert.equal(view.offset,25);
assert.equal(view.hasPrev,true);assert.equal(view.hasNext,true);
assert.equal(view.rows[0].net,14.3);assert.equal(view.rows[0].commission,-.7);
assert.equal(view.rows[1].net,null);
assert.equal(view.rows[1].commission,null,'no invented exact costs');
assert.equal(view.rows[0].brain,'system');
const last=out.journalEvidence(status,execution,{...source,offset:50},50,25);
assert.equal(last.ready,true);assert.equal(last.hasNext,false);
assert.equal(last.hasPrev,true);
for(const [name,st,bound,data,offset,limit] of [
 ['changed account',{...status,telemetry:{...t,login:101}},execution,source,25,25],
 ['switched REAL',{...status,telemetry:{...t,demo:false,account_trade_mode:2}},execution,source,25,25],
 ['Core not bound',status,{...execution,account_matches:false},source,25,25],
 ['wrong bound target',status,{...execution,settings:{...execution.settings,target:'live'}},source,25,25],
 ['offline broker',{...status,gateway_fresh:false},execution,source,25,25],
 ['history other account',status,execution,{...source,context:'other'},25,25],
 ['stale page 0',status,execution,source,0,25],
 ['excessive offset',status,execution,source,1_000_001,25],
 ['limit injection',status,execution,source,25,101],
 ['wrong symbol',status,execution,{...source,rows:[{...trade,symbol:'EURUSD'}]},25,25],
 ['unknown state',status,execution,{...source,rows:[{...trade,status:'PROFIT'}]},25,25]
 ]){
 const v=out.journalEvidence(st,bound,data,offset,limit);
 assert.equal(v.ready,false,name);assert.equal(v.rows.length,0,name);
 assert.equal(v.total,null,name);
}
const cmp=fs.readFileSync(path.join(root,'trade-journal-operations.tsx'),'utf8');
const bridge=fs.readFileSync(path.join(root,'bridge.ts'),'utf8');
const native=fs.readFileSync(path.join(root,'../../tauri/src/bridge.rs'),'utf8');
const core=fs.readFileSync(path.join(root,'../../../crates/runtime/src/trade_journal.rs'),'utf8');
const terminal=fs.readFileSync(path.join(root,'terminal.tsx'),'utf8');
for(const x of [
 "readPanel('/v1/execution'),readTradeHistory(offset,limit)",
 "setOffset(next)","setLimit(Number(e.target.value));setOffset(0)",
 "journalEvidence(status,data.execution,data.history,offset,limit)",
 "aria-label=\"مراكز التداول الموثقة\"",
 "setData(null);setSelected(null);setError('')"]){
 assert(cmp.includes(x),'journal UI is not a real paginated Core panel: '+x);
}
assert(bridge.includes("invoke('read_trade_history',{offset,limit})"));
assert(native.includes('open_client()?.post("/v1/execution/trades",json!({"offset":offset,"limit":limit}))'));
assert(native.includes('offset<=1_000_000 && (1..=100).contains(&limit)'));
assert(native.includes('get_status, read_panel, read_trade_history, execute_control'));
assert(core.includes('"context":context,"total":total'));
assert(terminal.includes('<TradeJournalOperations key={viewScope}'));
assert(terminal.includes("page==='journal'?"));
console.log('PASS G3-B: native read-only bounded MT5 journal pagination, actual 73-row navigation, exact account provenance, missing-fee redaction and Windows component wiring');
