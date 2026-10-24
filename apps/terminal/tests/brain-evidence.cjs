const assert=require('node:assert/strict');const fs=require('node:fs');const vm=require('node:vm');const ts=require('typescript');
const s=fs.readFileSync(require('node:path').join(__dirname,'../src/brain-evidence.ts'),'utf8');const out=ts.transpileModule(s,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020}});const e={};vm.runInNewContext(out.outputText,{exports:e});
const base={brains:[{config:{id:'system'},quality:{source:'Closed MT5 DEMO deal records; net=gross+commission+swap+fee',paper_and_forward_excluded:true,all:{samples:25,wins_after_costs:14,win_rate:14/25,net_booked:42}}}]};
assert.equal(e.brokerEvidence(base).samples,25);assert.equal(e.brokerEvidence(base).losses,11);assert(Math.abs(e.brokerEvidence(base).winRate-56)<1e-8);
assert.equal(e.brokerEvidence({brains:[{config:{id:'system'},paper_stats:{closed:100,wins:100}}]}),null);
assert.equal(e.brokerEvidence({brains:[{config:{id:'system'},quality:{...base.brains[0].quality,source:'paper',all:base.brains[0].quality.all}}]}),null);
assert.equal(e.brokerEvidence({brains:[{config:{id:'system'},quality:{...base.brains[0].quality,all:{samples:25,wins_after_costs:18,win_rate:14/25,net_booked:42}}}]}),null);
console.log('PASS: win rate and wins come only from exact closed and costed MT5 DEMO evidence');
