const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const assert=require('node:assert/strict'),ts=require('typescript');
const source=fs.readFileSync(path.join(__dirname,'../src/micro-readiness.ts'),'utf8');
const transpiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});
const mod={};vm.runInNewContext(transpiled.outputText,{exports:mod});
const {microAttestation}=mod;
const NOW=1780000000000;
const t={connected:true,demo:false,account_trade_mode:2,login:50,server:'Broker-REAL',currency:'USD',equity:50};
const status={gateway_fresh:true,telemetry:t};
const id={login:50,server:'Broker-REAL',currency:'USD',mode:2};
const audit=(state='candidate_only')=>({
 schema:'p1e-micro-recovery-v1',read_only:true,broker_execution_authorized:false,
 observed_ms:NOW,account:{...id},state,candidate_only:state==='candidate_only',
 configured:state!=='disabled',recovered_persisted:state==='recovered_latched',
 current_milestone_reached:state==='milestone_reached',
 cap_percent:10,recovery_equity:500,reached_ms:state==='recovered_latched'?NOW:null});
const monitor=(state='candidate_only')=>({state:'ready',enabled:true,locked:false,
 observed_ms:NOW,reason:'daily account breaker monitored',account:{...id},micro_real:audit(state)});
let result=microAttestation(status,monitor(),NOW);
assert.equal(result.verified,true);
assert.equal(result.candidate,true);
assert.equal(result.state,'candidate_only');
assert.equal(result.capPercent,10);
assert.equal(result.dailyState,'ready');
assert.equal(result.dailyEnabled,true);
assert.equal(result.recoveryEquity,500);
for(const mode of ['recovered_latched','milestone_reached','disabled']){
 result=microAttestation(status,monitor(mode),NOW);
 assert.equal(result.verified,true,mode);
 assert.equal(result.candidate,false,mode);
 assert.equal(result.state,mode);
}
result=microAttestation(status,{...monitor(),state:'locked',locked:true,reason:'daily loss exceeded'},NOW);
assert.equal(result.verified,true);
assert.equal(result.dailyState,'locked');
assert.equal(result.candidate,true,'micro candidate is independent of daily lock, NOT execution permission');
result=microAttestation(status,{...monitor(),state:'disabled',enabled:false},NOW);
assert.equal(result.verified,true);
assert.equal(result.dailyEnabled,false);
assert.match(result.dailyReason,/مطفأ/);
for(const [label,m,s,now] of [
 ['stale Core snapshot',monitor(),status,NOW+20_000],
 ['future Core clock',{...monitor(),observed_ms:NOW+3_000},status,NOW],
 ['other login',{...monitor(),account:{...id,login:51}},status,NOW],
 ['other server',{...monitor(),micro_real:{...audit(),account:{...id,server:'Stranger'}}},status,NOW],
 ['other mode',{...monitor(),micro_real:{...audit(),account:{...id,mode:0}}},status,NOW],
 ['other currency',{...monitor(),account:{...id,currency:'EUR'}},status,NOW],
 ['nonread-only',{...monitor(),micro_real:{...audit(),read_only:false}},status,NOW],
 ['faked execution permission',{...monitor(),micro_real:{...audit(),broker_execution_authorized:true}},status,NOW],
 ['source revision',{...monitor(),micro_real:{...audit(),schema:'unexpected'}},status,NOW],
 ['latch inconsistent',{...monitor(),micro_real:{...audit('candidate_only'),recovered_persisted:true}},status,NOW],
 ['empty monitor',{},status,NOW],
 ['disconnected',monitor(),{...status,telemetry:{...t,connected:false}},NOW],
 ['gateway stale',monitor(),{...status,gateway_fresh:false},NOW],
 ['Demo substitution',monitor(),{...status,telemetry:{...t,demo:true,account_trade_mode:0}},NOW],
 ['invalid timestamp',{...monitor(),observed_ms:NaN},status,NOW],
 ['invalid cap',{...monitor(),micro_real:{...audit(),cap_percent:250}},status,NOW],
 ]){
 const out=microAttestation(s,m,now);
 assert.equal(out.verified,false,label);
 assert.equal(out.candidate,false,label);
 assert.equal(out.capPercent,null,label);
}
const demo={...t,demo:true,account_trade_mode:0};
const demoId={...id,mode:0};
const demoMonitor={state:'disabled',enabled:false,observed_ms:NOW,reason:'disabled',account:demoId,
 micro_real:{...audit('unverified_or_demo'),account:demoId,candidate_only:false}};
const demoOut=microAttestation({gateway_fresh:true,telemetry:demo},demoMonitor,NOW);
assert.equal(demoOut.verified,true);assert.equal(demoOut.candidate,false);assert.equal(demoOut.label,'DEMO');
console.log('PASS: broker/account/time-scoped Core Micro recovery and daily breaker audit with irreversible latch and 16 fail-closed cases');
