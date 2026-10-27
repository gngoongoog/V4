// Consume Rust-generated data. Never substitute a hand-authored JSON fixture.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const ts=require('typescript');
assert(process.argv[2],'Pass the JSON produced by the Rust dashboard contract test');
const packet=JSON.parse(fs.readFileSync(process.argv[2],'utf8'));
assert.equal(packet.synthetic,true);
assert.equal(packet.broker_orders,false);
assert.equal(packet.build.version,require('../package.json').version);
const source=fs.readFileSync(path.join(__dirname,'../src/brain-evidence.ts'),'utf8');
const js=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020}});
const loaded={};vm.runInNewContext(js.outputText,{exports:loaded});
const decode=loaded.brokerEvidence;
const evidence=decode(packet.report,packet.status);
assert(evidence,'Production dashboard must accept the actual scoped Rust report');
assert.equal(evidence.samples,34);
assert.equal(evidence.wins,20);
assert.equal(evidence.losses,9);
assert.equal(evidence.breakeven,5);
assert.equal(evidence.net,31);
assert(Math.abs(evidence.winRate-100*20/34)<1e-9);
assert.equal(evidence.wins+evidence.losses+evidence.breakeven,evidence.samples);
for(const mutate of [
 p=>{p.status.gateway_fresh=false},
 p=>{p.status.telemetry.login+=1},
 p=>{p.report.trading_mode='sniper'},
 p=>{p.report.brains[0].quality.schema='unknown'},
 p=>{p.report.brains[0].quality.all.breakeven_after_costs=6},
]){
 const invalid=JSON.parse(JSON.stringify(packet));mutate(invalid);
 assert.equal(decode(invalid.report,invalid.status),null);
}
console.log('PASS: actual Rust report -> production TypeScript dashboard; synthetic receipts, no broker orders');
