const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const ts=require('typescript');
function load(file){
 const code=fs.readFileSync(path.join(__dirname,'../src',file),'utf8');
 const built=ts.transpileModule(code,{compilerOptions:{
  target:ts.ScriptTarget.ES2020,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});
 const errors=(built.diagnostics||[]).filter(e=>e.category===ts.DiagnosticCategory.Error);
 assert.equal(errors.length,0,errors.map(e=>ts.flattenDiagnosticMessageText(e.messageText,'\n')).join('\n'));
 const exports={};vm.runInNewContext(built.outputText,{exports});
 return exports;
}
const {feedEvidence}=load('feed.ts');
const {diagnoseMarketQuote}=load('market-quote-diagnosis.ts');
const now=1782000000000;
const broker={
 gateway_fresh:true,last_received_ms:now-200,
 telemetry:{connected:true,symbol:'XAUUSDr',error:'',
  balance:250,equity:250,login:55,server:'Demo',tick_age_ms:100}};
function diagnosis(status,lastStatusAt=now){
 return diagnoseMarketQuote(status,feedEvidence(status,true,lastStatusAt,now));
}
let r=diagnosis(broker);
assert.equal(r.code,'FRESH');
let missing={...broker,telemetry:{...broker.telemetry,
 error:'Symbol XAUUSDr is unavailable; choose its exact broker name in Settings.',
 tick_age_ms:null,bid:null,ask:null}};
r=diagnosis(missing);
assert.equal(r.code,'SYMBOL_MISSING');
assert.equal(r.settingsRequired,true);
assert.match(r.detail,/XAUUSDr/);
assert.match(r.detail,/لن نبدّل الرمز تلقائياً/);
const wrongSymbol={...missing,telemetry:{...missing.telemetry,
 error:'Symbol XAUUSD is unavailable; choose its exact broker name in Settings.'}};
assert.equal(diagnosis(wrongSymbol).code,'NO_QUOTE',
 'A stale error for another symbol cannot accuse the current exact MT5 symbol');
const old={...broker,telemetry:{...broker.telemetry,
 error:'',tick_age_ms:3605000}};
r=diagnosis(old);
assert.equal(r.code,'STALE_QUOTE');
assert.match(r.detail,/ساعة/);
assert.match(r.detail,/ربما السوق مغلق/);
assert.match(r.detail,/لا يمكن إثبات السبب/);
assert.equal(r.settingsRequired,false);
const missingClock={...broker,telemetry:{...broker.telemetry,
 tick_age_ms:null,bid:1999,ask:2000}};
assert.equal(diagnosis(missingClock).code,'UNVERIFIED_CLOCK');
assert.equal(diagnosis({...broker,gateway_fresh:false}).code,'BROKER_OFFLINE');
assert.equal(diagnosis(broker,now-7000).code,'CORE_OFFLINE');
assert.equal(diagnosis({...broker,telemetry:{...broker.telemetry,tick_age_ms:null,bid:null,ask:null}}).code,'NO_QUOTE');
assert.equal(feedEvidence(old,true,now,now).quote,false,'Stale broker price must not become trade eligible');
console.log('PASS P0-E2: exact symbol diagnostics, after-hours stale quote, absent ticks, unknown clock, no false offline or fresh price');
