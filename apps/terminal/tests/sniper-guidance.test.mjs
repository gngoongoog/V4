import assert from 'node:assert/strict';
import {adviceForSniperCode,classifySniperCauses} from '../src/sniper-guidance.ts';

const rows=[
 {code:'setup_not_formed',label_ar:'لا توجد إشارة',sampled_decisions_with_cause:100},
 {code:'unsettled_intent',label_ar:'تسوية أوامر',sampled_decisions_with_cause:2},
 {code:'stale_quote',label_ar:'سعر قديم',sampled_decisions_with_cause:25},
 {code:'stop_risk_budget',label_ar:'خطر مرتفع',sampled_decisions_with_cause:6},
 {code:'mt5_trading_disabled',label_ar:'صلاحية MT5',sampled_decisions_with_cause:3},
 {code:'setup_not_formed',label_ar:'مكرر',sampled_decisions_with_cause:300}
];
const ordered=classifySniperCauses(rows);
assert.deepEqual(ordered.map(x=>x.code),[
 'unsettled_intent','stale_quote','mt5_trading_disabled','stop_risk_budget','setup_not_formed'
]);
assert.equal(ordered[0].count,2,'Do not double count duplicated code');
assert.match(ordered[0].advice.action,/لا تعد الإرسال/);
assert.equal(adviceForSniperCode('setup_not_formed').category,'expected');
assert.equal(adviceForSniperCode('unknown-new-gate').category,'unknown');
assert.equal(adviceForSniperCode('__proto__').category,'unknown','No inherited names');
assert.deepEqual(classifySniperCauses({not:'an array'}),[]);
assert.deepEqual(classifySniperCauses([{code:'unsettled_intent',sampled_decisions_with_cause:NaN}]),[]);
assert.deepEqual(classifySniperCauses([{code:'unsettled_intent',sampled_decisions_with_cause:-1}]),[]);
assert(classifySniperCauses([{code:'new_reason',sampled_decisions_with_cause:1}])[0].advice.action.includes('السجل'));
console.log('PASS: Sniper diagnostic guidance is read-only, deterministic, deduplicated and conservative');
