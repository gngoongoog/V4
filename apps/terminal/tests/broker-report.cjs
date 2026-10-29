const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require('typescript'),root=path.resolve(__dirname,'../src');
const load=file=>ts.transpileModule(fs.readFileSync(path.join(root,file),'utf8'),{
 fileName:file,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}}).outputText;
const logic={};vm.runInNewContext(load('broker-report.ts'),{exports:logic,Number,Array,Object});
const status={gateway_fresh:true,telemetry:{connected:true,demo:true,account_trade_mode:0,login:17,currency:'USD'}};
const item=(id,net,cost=true,closed=true)=>({
 positionId:id,symbol:'XAUUSD',side:'BUY',status:closed?'CLOSED':'OPEN',
 lotsOpen:.1,lotsClosed:closed?.1:0,remaining:closed?0:.1,
 net:cost?net:null,profit:cost?net+.7:null,commission:cost?-.7:null,
 fee:cost?0:null,swap:cost?0:null,provenCosts:cost,brain:'system',mode:'sniper',entryPrice:2000,latestDeal:'2026-10-29 12:00:00 UTC',reason:'test',opened:'now'
});
const make=(rows,total=rows.length)=>({ready:true,reason:'broker verified',
 rows,total,offset:0,limit:100,hasPrev:false,hasNext:total>100});
const {brokerReport}=logic;
const rows=Array.from({length:32},(_,i)=>item(1000+i,i<25?1:-2));
rows.push(item(2000,990,false),item(2001,400,true,false));
const sample=brokerReport(status,make(rows,152));
assert.equal(sample.ready,true);
assert.equal(sample.mode,'DEMO');
assert.equal(sample.fullyClosed,32);
assert.equal(sample.unverified,2);
assert.equal(sample.sampled,34);
assert.equal(sample.allPositions,152);
assert.equal(sample.complete,false,'latest 100 sample must not imply lifetime statement');
assert.equal(sample.wins,25);assert.equal(sample.losses,7);
assert.equal(sample.winRate,25/32*100);
assert(Math.abs(sample.net-11)<1e-8);
assert(Math.abs(sample.commission-(-22.4))<1e-8);
const tiny=brokerReport(status,make([item(1,2),item(2,-1)]));
assert.equal(tiny.winRate,null,'not a confident strategy win rate with 2 broker trades');
assert.equal(tiny.complete,true);
const none=brokerReport(status,make([item(1,40,false),item(2,40,true,false)]));
assert.equal(none.ready,true);assert.equal(none.net,null);assert.equal(none.wins,null);
const disconnected=brokerReport({...status,gateway_fresh:false},make(rows));
assert.equal(disconnected.ready,false);assert.equal(disconnected.net,null);
const different=brokerReport(status,{...make(rows),ready:false,reason:'Other MT5 account'});
assert.equal(different.ready,false);assert.equal(different.net,null);
const real=brokerReport({...status,telemetry:{...status.telemetry,demo:false,account_trade_mode:2}},make([item(1,5)]));
assert.equal(real.mode,'REAL');
const generated={};const jsx=(type,props)=>({type,props:props||{}});
const series={};vm.runInNewContext(load('report-series.ts'),{exports:series,Number,Date,Array,Object});
const csv={};vm.runInNewContext(load('journal-export.ts'),{exports:csv,String,TextEncoder,Uint8Array,DataView});
let initialSlot=null;const saved=[];
vm.runInNewContext(load('broker-report-operations.tsx'),{exports:generated,
 require:id=>{
  if(id==='react')return {useState:init=>[init===null?initialSlot:init,()=>{}],useEffect:()=>{},useMemo:fn=>fn()};
  if(id==='react/jsx-runtime')return {jsx,jsxs:jsx};
  if(id==='lucide-react')return Object.fromEntries(['BarChart3','CalendarDays','Download','FileText','Filter','Printer','RefreshCw','Target','TrendingUp'].map(x=>[x,x]));
  if(id==='./broker-report')return logic;
  if(id==='./report-series')return series;
  if(id==='./journal-export')return csv;
  if(id==='./journal-evidence')return {journalEvidence:()=>make(rows,152)};
  if(id==='./bridge')return {hasTauri:()=>true,saveExcelReport:async(bytes,kind)=>{saved.push({bytes,kind});return {saved:true}},readPanel:()=>Promise.reject(Error('offline')),readTradeHistory:()=>Promise.reject(Error('offline'))};
  throw Error('Unexpected module '+id);
 },Date,Number,Object,Array,console});
const element=generated.BrokerReportOperations({status,scope:'verified',preview:false,onJournal:()=>{}});
assert.equal(element.props['aria-label'],'Verified MT5 broker performance report');
function nodes(n,out=[]){if(!n||typeof n!=='object')return out;if(Array.isArray(n)){n.forEach(v=>nodes(v,out));return out}out.push(n);nodes(n.props?.children,out);return out}
initialSlot={scope:'verified',execution:{},journal:{}};
const ready=generated.BrokerReportOperations({status,scope:'verified',preview:false,onJournal:()=>{}});
const excel=nodes(ready).find(n=>n.type==='button'&&String(n.props.children).includes('تصدير Excel'));
assert(excel&&!excel.props.disabled,'Verified sample must offer the native Excel export');
excel.props.onClick();assert.equal(saved.length,1);assert.equal(saved[0].kind,'report');
assert.equal(Buffer.from(saved[0].bytes).readUInt32LE(0),0x04034b50,'Native save receives an XLSX archive');
const component=fs.readFileSync(path.join(root,'broker-report-operations.tsx'),'utf8');
const main=fs.readFileSync(path.join(root,'terminal.tsx'),'utf8');
assert(component.includes("readTradeHistory(0,100)")&&component.includes("journalEvidence(status,slot.execution,slot.journal,0,100)"));
assert(component.includes('آخر 100 مركز'));
assert(component.includes('لا تظهر نسبة النجاح إلا بعد 30 مركزاً'));
assert(main.includes("page==='reports'?<div className"));
assert(main.includes('<BrokerReportOperations key={viewScope}'));
assert(main.includes("route:'/v1/execution/trades'"));
assert(!main.includes("page==='reports'?<DataExplorer"));
console.log('PASS G3-C: verified closed MT5 costed reports, 100-sample honest scope, fee redaction, >=30 winner threshold, REAL/DEMO, source-only React wiring');
