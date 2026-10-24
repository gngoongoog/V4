const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const path=require('node:path');
const ts=require('typescript');
const file=path.join(__dirname,'../src/system-health.tsx');
const source=fs.readFileSync(file,'utf8');
const feedSource=fs.readFileSync(path.join(__dirname,'../src/feed.ts'),'utf8');
const feedOut=ts.transpileModule(feedSource,{compilerOptions:{target:ts.ScriptTarget.ES2020,module:ts.ModuleKind.CommonJS}});
const feedExports={};vm.runInNewContext(feedOut.outputText,{exports:feedExports});
const out=ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2020,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX},reportDiagnostics:true});
const issues=(out.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error);
assert.equal(issues.length,0,issues.map(x=>ts.flattenDiagnosticMessageText(x.messageText,'\n')).join('\n'));
const moduleExports={};
const sandbox={exports:moduleExports,require:(id)=>{
  if(id==='react/jsx-runtime')return {jsx:(type,props)=>({type,props}),jsxs:(type,props)=>({type,props})};
  if(id==='./feed')return feedExports;
  if(id==='lucide-react')return Object.fromEntries(['Activity','ArrowUpRight','Clock3','Cpu','DatabaseZap','ShieldCheck','Wifi','Workflow'].map(name=>[name,name]));
  throw Error('Unexpected browser module '+id);
}};
vm.runInNewContext(out.outputText,sandbox,{filename:'system-health.cjs'});
const health=moduleExports.buildHealth;
const now=1782000000000;
const fresh={version:'v4',last_received_ms:now-100,gateway_fresh:true,execution_enabled:false,requested_run:false,telemetry:{connected:true,tick_age_ms:118}};
function check(s,online,preview,risk,at){return health(s,online,preview,risk,at,now)}
function row(v,id){return v.rows.find(r=>r.key===id)}
let v=check(fresh,true,true,true,now);
assert.equal(v.title,'DESIGN PREVIEW');assert.equal(v.tone,'preview');assert.equal(row(v,'execution').value,'LOCKED');
v=check(null,false,false,false,null);
assert.equal(v.title,'CORE OFFLINE');assert.equal(row(v,'mt5').tone,'bad');assert.equal(v.quoteAge,'—');
v=check(fresh,true,false,true,now-5200);
assert.equal(v.title,'CORE OFFLINE');assert.equal(row(v,'core').value,'OFFLINE');
v=check({...fresh,gateway_fresh:false},true,false,true,now);
assert.equal(v.title,'FEED OFFLINE');assert.equal(row(v,'quote').value,'UNAVAILABLE');
v=check({...fresh,telemetry:{...fresh.telemetry,tick_age_ms:3100}},true,false,true,now);
assert.equal(v.title,'QUOTE STALE');assert.equal(row(v,'quote').value,'UNAVAILABLE');
v=check(fresh,true,false,false,now);
assert.equal(v.title,'MONITORING');assert.equal(row(v,'execution').value,'STOPPED');assert.equal(row(v,'risk').value,'UNKNOWN');assert.equal(v.quoteAge,'218 ms');
v=check({...fresh,requested_run:true,execution_enabled:false},true,false,true,now);
assert.equal(row(v,'execution').value,'BLOCKED');
v=check({...fresh,requested_run:true,execution_enabled:true},true,false,true,now);
assert.equal(v.title,'EXECUTION ACTIVE');assert.equal(row(v,'execution').value,'ARMED');assert.equal(row(v,'risk').value,'LOADED');
v=check({...fresh,telemetry:{...fresh.telemetry,tick_age_ms:null}},true,false,true,now);
assert.equal(row(v,'quote').value,'UNAVAILABLE');
v=check({...fresh,last_received_ms:now-3500},true,false,true,now);
assert.equal(v.title,'QUOTE STALE','Core heartbeats must not refresh old broker quotes');
assert.equal(row(v,'quote').value,'UNAVAILABLE');
v=check({...fresh,last_received_ms:null},true,false,true,now);
assert.equal(row(v,'quote').value,'UNAVAILABLE','missing timestamp cannot authorize an MT5 quote');
v=check(fresh,true,false,true,now);
assert.equal(v.rows.length,5);
assert(v.rows.every(r=>['events','settings','analysis','execution','risk'].includes(r.page)));
console.log('PASS: preview isolation; offline core/MT5; stale Core stream; stale market price; real execution & risk states; correct nav');

const nav=[];
const tree=moduleExports.default({health:v,onOpen:page=>nav.push(page)});
const buttons=[];
function visit(node){if(!node||typeof node!=='object')return;
 if(Array.isArray(node)){node.forEach(visit);return;}
 if(node.type==='button')buttons.push(node);
 visit(node.props?.children);
}
visit(tree);
assert.equal(buttons.length,6); // title opens events + five individually routed signals
buttons.forEach(b=>b.props.onClick());
assert.deepEqual(nav,['events','events','settings','analysis','execution','risk']);
console.log('PASS: health card header + all five signal links invoke the correct V4 native pages');
