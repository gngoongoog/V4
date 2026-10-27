const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require('typescript'),base=path.resolve(__dirname,'../src');
const transpile=f=>ts.transpileModule(fs.readFileSync(path.join(base,f),'utf8'),{
 fileName:f,compilerOptions:{module:ts.ModuleKind.CommonJS,
 target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX},reportDiagnostics:true}).outputText;
const micro={};vm.runInNewContext(transpile('micro-readiness.ts'),{exports:micro,Number,Object,JSON});
const logic={};vm.runInNewContext(transpile('daily-risk-evidence.ts'),{exports:logic,
 require:id=>{if(id==='./micro-readiness')return micro;throw Error('Unexpected module '+id)},
 Number,Object,Math,JSON});
const now=1780000000000;
const telemetry={connected:true,login:100,server:'DEMO SERVER',currency:'USD',
 symbol:'XAUUSD',demo:true,account_trade_mode:0};
const status={gateway_fresh:true,telemetry};
const account={login:telemetry.login,server:telemetry.server,currency:telemetry.currency,mode:0};
const microInfo={schema:'p1e-micro-recovery-v1',read_only:true,broker_execution_authorized:false,
 observed_ms:now,account:{...account},state:'unverified_or_demo',configured:false,
 candidate_only:false,recovered_persisted:false,current_milestone_reached:false,
 cap_percent:10,recovery_equity:500};
const monitor={state:'ready',enabled:true,locked:false,
 reason:'daily verified',observed_ms:now,account:{...account},micro_real:microInfo,
 day:'2026-05-28',limit_percent:3,data_valid:true,estimated_day_start_balance:100,
 estimated_day_loss:1,estimated_day_loss_percent:1,allowed_day_loss:3};
const view=(s=status,m=monitor,n=now)=>logic.dailyRiskView(s,m,n);
let a=view();
assert.equal(a.verified,true);assert.equal(a.mode,'DEMO');
assert.equal(a.dayLoss,1);assert.equal(a.allowedLoss,3);assert.equal(a.remaining,2);
assert.equal(a.persistedLock,false);assert.equal(a.limitPercent,3);
let locked=view(status,{...monitor,state:'locked',locked:true,estimated_day_loss:5,estimated_day_loss_percent:5});
assert.equal(locked.state,'locked');assert.equal(locked.remaining,0);assert.equal(locked.persistedLock,true);
const breached=view(status,{...monitor,state:'breached',estimated_day_loss:4,estimated_day_loss_percent:4});
assert.equal(breached.verified,true);assert.equal(breached.remaining,0);
const disabled=view(status,{...monitor,state:'disabled',enabled:false,locked:true});
assert.equal(disabled.verified,true);assert.equal(disabled.remaining,null);
assert.equal(disabled.persistedLock,true);
assert(disabled.warning.includes('قفل سابق'));
const missingMath=view(status,{...monitor,data_valid:false,estimated_day_loss:null});
assert.equal(missingMath.verified,false,'ready with missing data is not attested');
const lockedMissingMath=view(status,{...monitor,state:'locked',locked:true,data_valid:false,
 estimated_day_start_balance:null,estimated_day_loss:null,allowed_day_loss:null});
assert.equal(lockedMissingMath.verified,true,'durable lock remains factual without daily budget math');
assert.equal(lockedMissingMath.remaining,null);
for(const [label,s,m,n] of [
 ['other account',status,{...monitor,account:{...account,login:999}},now],
 ['other broker',status,{...monitor,micro_real:{...microInfo,account:{...account,server:'OTHER'}}},now],
 ['REAL substitution',{...status,telemetry:{...telemetry,demo:false,account_trade_mode:2}},monitor,now],
 ['stale account feed',{...status,gateway_fresh:false},monitor,now],
 ['stale Core report',status,monitor,now+20_000],
 ['future Core clock',status,{...monitor,observed_ms:now+3000},now],
 ['invalid day',status,{...monitor,day:'bad'},now],
 ['invalid limit',status,{...monitor,limit_percent:0},now],
 ['wrong daily allowance',status,{...monitor,allowed_day_loss:40},now],
 ['conflicting ready lock',status,{...monitor,locked:true},now],
 ['conflicting locked without lock',status,{...monitor,state:'locked',locked:false},now],
 ['conflicting disabled enabled',status,{...monitor,state:'disabled',enabled:true},now],
 ['missing micro attestation',status,{...monitor,micro_real:null},now],
 ['bad read-only contract',status,{...monitor,micro_real:{...microInfo,read_only:false}},now]
 ]){
  const v=view(s,m,n);
  assert.equal(v.verified,false,label);assert.equal(v.remaining,null,label);
  assert.equal(v.dayLoss,null,label);assert.equal(v.accountCurrency,null,label);
}
const realTelemetry={...telemetry,account_trade_mode:2,demo:false};
const realId={...account,mode:2};
const realMicro={...microInfo,account:realId,state:'disabled',configured:false};
const realMonitor={...monitor,account:realId,micro_real:realMicro};
const real=view({...status,telemetry:realTelemetry},realMonitor);
assert.equal(real.verified,true);assert.equal(real.mode,'REAL');
const renderer={};const jsx=(type,props)=>({type,props:props||{}});
vm.runInNewContext(transpile('daily-risk-operations.tsx'),{exports:renderer,
 require:id=>{
  if(id==='./daily-risk-evidence')return logic;
  if(id==='react/jsx-runtime')return {jsx,jsxs:jsx};
  throw Error('Module '+id);
 },Date,Number,Object,Math,JSON});
const widget=renderer.DailyRiskOperations({status,monitor,preview:false});
assert.equal(widget.props['aria-label'],'Verified account daily-loss breaker');
const visual=fs.readFileSync(path.join(base,'daily-risk-operations.tsx'),'utf8');
const terminal=fs.readFileSync(path.join(base,'terminal.tsx'),'utf8');
assert(visual.includes('dailyRiskView(status,monitor,Date.now())'));
assert(visual.includes('القاطع:'));
assert(visual.includes('DESIGN PREVIEW'));
assert(terminal.includes('<DailyRiskOperations status={!preview&&feed.broker?status:null} monitor={safeRiskSnapshot} preview={preview}/>'));
console.log('PASS G4-B: true all-symbol MT5 daily breaker state, persisted lock, day-loss budget, stale/mode guards and functional Windows risk panel');
