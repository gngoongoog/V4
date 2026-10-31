const fs=require('node:fs'),assert=require('node:assert/strict'),vm=require('node:vm'),path=require('node:path');
const ts=require('typescript');
const code=fs.readFileSync(path.join(__dirname,'../src/terminal.tsx'),'utf8');
const feed=fs.readFileSync(path.join(__dirname,'../src/feed.ts'),'utf8');
const compiled=ts.transpileModule(feed,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020}}).outputText;
const exported={};vm.runInNewContext(compiled,{exports:exported});
const now=1782000000000;
const status={
 gateway_fresh:true,last_received_ms:now-100,
 telemetry:{connected:true,demo:true,account_trade_mode:0,login:12,server:'Demo',
  currency:'USD',balance:1000,equity:990,tick_age_ms:60000}
};
const closed=exported.feedEvidence(status,true,now,now);
assert.equal(closed.core,true);
assert.equal(closed.broker,true,'a broker account snapshot can remain current while a symbol has no new ticks');
assert.equal(closed.quote,false,'trading must remain blocked without a fresh market quote');
assert.match(code,/const trustworthy=preview\|\|feed\.quote;/,'price-dependent execution gates retain the 3-second quote requirement');
assert.match(code,/const accountFresh=preview\|\|feed\.broker;/,'balance must depend on verified broker telemetry, not tick freshness');
assert.match(code,/const account=accountFresh\?t:null;/,'stale market ticks must not erase an authenticated Demo account');
assert.match(code,/const accountStats=preview\?null:account;/,'preview cannot leak example account figures into live statistics');
assert.match(code,/\['Balance',money\(accountStats\?\.balance,currency\)\]/);
assert.match(code,/\['Equity',money\(accountStats\?\.equity,currency\)\]/);
assert.match(code,/\['Margin',money\(accountStats\?\.margin,currency\)\]/,
  'never hardcode used margin to a dash, only show the MT5-reported value');
assert.doesNotMatch(code,/\['Margin','—'\]/);
const gateway=fs.readFileSync(path.join(__dirname,'../../../gateway/mt5_gateway.py'),'utf8');
assert.match(gateway,/raw_margin = getattr\(account, "margin", None\)/);
assert.match(gateway,/leverage=leverage, margin=margin, margin_free=margin_free/);
assert.match(gateway,/margin=None, volume_min=None/);

assert.match(code,/ready=\{!preview&&trustworthy\}/,'the execution binding readiness may not become tickless');
assert(code.includes("className={'pulse-dot '+(accountFresh?'online':'')}"),'connection pill must show broker status independently of tick status');
assert.match(code,/accountFresh\?'MT5 CONNECTED':'MT5 OFFLINE'/);
assert.match(code,/feed\.broker\?\(feed\.quote\?'ACCOUNT \+ QUOTES FRESH':'ACCOUNT CONNECTED · QUOTES STALE'\)/);
const disconnected=exported.feedEvidence({...status,gateway_fresh:false},true,now,now);
assert.equal(disconnected.broker,false);
assert.equal(disconnected.quote,false);
console.log('PASS: Sunday closed-market Demo account balance/equity visible only with broker freshness; tickless trading remains blocked');
