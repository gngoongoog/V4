const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const root=path.resolve(__dirname,'../../..');
const code=p=>fs.readFileSync(path.join(root,p),'utf8');
test('P1-K5B is frozen research directional markout with hard non-PnL semantics',()=>{
 const audit=code('crates/runtime/src/sniper_forward_direction.rs');
 const study=code('crates/runtime/src/sniper_frozen_tick.rs');
 assert.match(audit,/HORIZON_MS:u64=5_000/);
 assert.match(audit,/MIN_REPORTED:u64=100/);
 assert.match(audit,/self\.pending\.take\(\)/);
 assert.match(audit,/no overlapping/i);
 assert.match(audit,/independent_oos":false/);
 assert.match(audit,/net_pnl":Value::Null/);
 assert.match(study,/trial\.forward\.invalidate_segment\(\)/);
 assert.match(study,/forward_direction_audit":trial\.forward\.json\(\)/);
 assert.match(study,/adaptive\.is_some_and\(\|x\|x\.key==frozen\.key\)/);
});
test('P1-K5B UI never markets markout fraction as filled-trade winrate',()=>{
 const ui=code('apps/terminal/src/sniper-frozen-operations.tsx');
 assert.match(ui,/forward\.schema==='p1k5b-forward-direction-v1'/);
 assert.match(ui,/forward\.net_pnl===null/);
 assert.match(ui,/forward\.profitability_validated===false/);
 assert.match(ui,/forward\.independent_oos===false/);
 assert.match(ui,/نسبة مطابقة اتجاه وصفية/);
 assert.match(ui,/مو PnL أو Win Rate/);
});

test('P1-K5C requires checked frozen Demo training cutoff without OOS claim',()=>{
 const learner=code('crates/runtime/src/sniper_learning.rs');
 const study=code('crates/runtime/src/sniper_frozen_tick.rs');
 const service=code('crates/runtime/src/execution.rs');
 const ui=code('apps/terminal/src/sniper-frozen-operations.tsx');
 assert.match(learner,/pub fn frozen_training_checkpoint\(/);
 assert.match(learner,/model_count==count&&model_targets==targets/);
 assert.match(learner,/Future-dated broker close cannot enter training cutoff/);
 assert.match(study,/fn freeze_verified\(/);
 assert.match(study,/training_checkpoint":trial\.training_checkpoint/);
 assert.match(service,/sniper_learning::frozen_training_checkpoint\(/);
 assert.match(service,/sniper_frozen_tick\.freeze_verified\(/);
 assert.match(ui,/training\.status==='captured'/);
 assert.match(ui,/training\.broker_pnl===null/);
 assert.match(ui,/training\.independent_oos===false/);
});
