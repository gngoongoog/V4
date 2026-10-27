const assert=require('node:assert/strict');const fs=require('node:fs');const vm=require('node:vm');const path=require('node:path');const ts=require('typescript');
function load(file){const source=fs.readFileSync(path.join(__dirname,'../src',file),'utf8');const out=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020},reportDiagnostics:true});const errors=(out.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error);assert.equal(errors.length,0);const exports={};vm.runInNewContext(out.outputText,{exports});return exports;}
const {coreGroups,searchRows,visibleColumns}=load('data-shape.ts');const {feedEvidence}=load('feed.ts');
let groups=coreGroups({events:[{kind:'START',timestamp_ms:100},{kind:'STOP',timestamp_ms:200}],positions:[{symbol:'XAUUSD',profit:8}]});
assert.deepEqual([...groups.map(x=>x.name)],['events','positions']);
assert.equal(searchRows(groups[0].rows,'stop').length,1);assert.equal(searchRows(groups[0].rows,'nomatch').length,0);
assert.deepEqual([...visibleColumns(groups[0].rows)],['kind','timestamp_ms']);
assert.equal(coreGroups(null).length,0);
assert.equal(coreGroups({some:5,enabled:false,secret:{no:'nested-secret'}})[0].rows.length,2);
assert.equal(coreGroups({events:Array.from({length:320},(_,i)=>({id:i}))})[0].rows.length,150);
const now=1782000000000;const status={last_received_ms:now-100,telemetry:{connected:true,tick_age_ms:80},gateway_fresh:true};
let out=feedEvidence(status,true,now,now);assert.equal(out.core,true);assert.equal(out.broker,true);assert.equal(out.quote,true);assert.equal(out.quoteAgeMs,180);
out=feedEvidence({...status,last_received_ms:now-3100},true,now,now);assert.equal(out.core,true);assert.equal(out.broker,true);assert.equal(out.quote,false);
out=feedEvidence(status,true,now-5400,now);assert.equal(out.core,false);assert.equal(out.quote,false);
out=feedEvidence({...status,last_received_ms:undefined},true,now,now);assert.equal(out.quote,false);
out=feedEvidence({...status,telemetry:{connected:false,tick_age_ms:80}},true,now,now);assert.equal(out.broker,false);
console.log('PASS: real Core endpoint groups, search, pagination cap, stale broker quote and live heartbeat isolation');
