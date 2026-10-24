const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),ts=require('typescript');
const base=path.resolve(__dirname,'../src');
const transpile=p=>ts.transpileModule(fs.readFileSync(path.join(base,p),'utf8'),{
 compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}
}).outputText;
const logic={};vm.runInNewContext(transpile('execution-quality.ts'),{exports:logic,JSON,Set,Number,Object});
const {qualityObservation}=logic;
const NOW=Date.now();
const t={connected:true,demo:true,account_trade_mode:0,login:12345,server:'Demo-Srv',currency:'USD',symbol:'XAUUSD'};
const status={gateway_fresh:true,telemetry:t};
const baseReport={
 schema:'p1f-execution-quality-v1',state:'observed',read_only:true,account_verified:true,
 observed_ms:NOW,context:JSON.stringify([t.server,t.login,t.symbol,t.currency]),
 account:{server:t.server,login:t.login,symbol:t.symbol,currency:t.currency,mode:0},
 sample_size:3,rejected_count:1,unknown_count:1,unresolved_count:2,
 rejection_categories:{insufficient_margin:1},
 queue_to_send:{samples:1,p95_ms:14},send:{samples:1,p95_ms:5},total:{samples:1,p95_ms:25},
 rows:[{brain:'system',kind:'open',state:'rejected',category:'insufficient_margin',retcode:10019,created_ms:NOW-1000},
       {brain:'system',kind:'open',state:'unknown',category:'unknown_execution',retcode:null,created_ms:NOW-900}]
};
const o=qualityObservation(status,baseReport,NOW);
assert.equal(o.ready,true);
assert.equal(o.rejects,1);
assert.equal(o.unknown,1);
assert.equal(o.unresolved,2);
assert.equal(o.sendP95,5);
assert.equal(o.rows[0].retcode,10019);
for(const [name,source,state,now] of [
 ['missing report',null,status,NOW],
 ['stale report',baseReport,status,NOW+20_000],
 ['another broker server',{...baseReport,account:{...baseReport.account,server:'Other'}},status,NOW],
 ['another symbol',{...baseReport,context:JSON.stringify([t.server,t.login,'EURUSD',t.currency])},status,NOW],
 ['another account',{...baseReport,account:{...baseReport.account,login:44}},status,NOW],
 ['broken category total',{...baseReport,rejection_categories:{}},status,NOW],
 ['fabricated metrics',{...baseReport,total:{samples:1,p95_ms:NaN}},status,NOW],
 ['invalid row',{...baseReport,rows:[{...baseReport.rows[0],state:'profit'}]},status,NOW],
 ['gateway not fresh',baseReport,{...status,gateway_fresh:false},NOW],
 ['account changed',baseReport,{...status,telemetry:{...t,login:999}},NOW],
 ['real mode wrong',baseReport,{...status,telemetry:{...t,demo:false,account_trade_mode:2}},NOW],
]){
 const view=qualityObservation(state,source,now);
 if(name==='fabricated metrics'){
   assert.equal(view.ready,true,'missing invalid metrics are simply unavailable');
   assert.equal(view.totalP95,null);
 }else{
   assert.equal(view.ready,false,name);
   assert.equal(view.rejects,null,name);
 }
}
const tauri=fs.readFileSync(path.join(base,'../../tauri/src/bridge.rs'),'utf8');
const core=fs.readFileSync(path.join(base,'../../crates/runtime/src/execution.rs'),'utf8');
const mod=fs.readFileSync(path.join(base,'../../crates/runtime/src/execution_quality.rs'),'utf8');
const terminal=fs.readFileSync(path.join(base,'terminal.tsx'),'utf8');
for(const [label,src,token] of [
 ['tauri',tauri,'"/v1/execution/quality"'],
 ['core',core,'crate::execution_quality::report(db,t,now)'],
 ['core account guard',core,'status.gateway_fresh && self.account_matches(status)'],
 ['bounded exact context',mod,'WHERE context=?1 ORDER BY created_ms DESC,id DESC LIMIT 100'],
 ['backend sanitized',mod,'Broker comments may contain secrets or personal data'],
 ['frontend scoped',terminal,"scopedGeneration.current.beginRead('executionQuality')"],
 ['frontend no preview',terminal,'status={!preview&&feed.broker?status:null}'],
])assert(src.includes(token),label+' missed '+token);
const rendered={};const jsx=(type,props)=>({type,props:props||{}});
vm.runInNewContext(transpile('execution-quality-operations.tsx'),{
 exports:rendered,require:id=>{
   if(id==='react/jsx-runtime')return {jsx,jsxs:jsx};
   if(id==='./execution-quality')return logic;
   throw Error('Unexpected import '+id);
 },Date,Number,Object,JSON
});
function nodes(v,output=[]){
 if(!v||typeof v!=='object')return output;
 if(Array.isArray(v)){for(const part of v)nodes(part,output);return output;}
 output.push(v);nodes(v.props?.children,output);return output;
}
let refreshes=0;
const panel=rendered.ExecutionQualityOperations({status,report:baseReport,disabled:false,onRefresh:()=>refreshes++});
const list=nodes(panel);
const refresh=list.find(x=>x.type==='button'&&x.props.children==='تحديث جودة التنفيذ من Core');
assert(refresh,'refresh button actually rendered');
refresh.props.onClick();assert.equal(refreshes,1,'refresh button dispatches actual action');
assert(list.some(x=>x.type==='table'),'real broker journal table displayed');
const missing=rendered.ExecutionQualityOperations({status:null,report:baseReport,disabled:true,onRefresh:()=>refreshes++});
assert(!nodes(missing).some(x=>x.type==='table'),'stale/unbound must never show other broker order history');
console.log('PASS: Core journal-only execution diagnosis with account, mode, quote freshness, real P95, redaction and working refresh');
