const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const path=require('node:path');
const ts=require('typescript');
const root=path.resolve(__dirname,'..');
const read=(p)=>fs.readFileSync(path.join(root,p),'utf8');
const source=read('src/partial-exit-price-operations.tsx');
const compiled=ts.transpileModule(source,{fileName:'partial-exit-price-operations.tsx',
 compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,
  jsx:ts.JsxEmit.ReactJSX},reportDiagnostics:true});
assert.equal((compiled.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
const exp={},el=(type,props)=>({type,props:props||{}});
let selected='',inputs=['','',''],dirty=false,liveConfirm='',hook=0;
const useState=(init)=>{const slot=hook++;const value=slot===0?selected:slot===1?inputs:slot===2?dirty:liveConfirm;
 return [value,()=>{}]};
vm.runInNewContext(compiled.outputText,{
 exports:exp,require:(id)=>{
  if(id==='react/jsx-runtime')return {jsx:el,jsxs:el};
  if(id==='react')return {useState};
  throw Error('Unexpected module: '+id);
 },Number,Object,Array,JSON,Math,String
});
function collect(n,out=[]){
 if(n==null)return out;
 if(Array.isArray(n)){for(const k of n)collect(k,out);return out}
 if(typeof n==='object'){out.push(n);collect(n.props?.children,out);return out}
 out.push(n);return out;
}
const render=(opts)=>{hook=0;return exp.PartialExitPriceOperations({onSave:async()=>true,onConsent:async()=>true,onTradeApproval:async()=>true,...opts})};
const shown=(root)=>collect(rendered).filter(x=>typeof x==='string').join(' ');
const tel={connected:true,demo:true,account_trade_mode:0,margin_mode:2,
 login:17,server:'D',symbol:'XAUUSDm',currency:'USD',
 tick_size:0.01,bid:2000,ask:2000.2,
 positions:[{identifier:907,ticket:87,magic:410601,side:'BUY',
 symbol:'XAUUSDm',price_open:1999.5,volume:0.1,tp:0}]};
const status={gateway_fresh:true,requested_run:false,telemetry:tel};
const context=JSON.stringify([tel.server,tel.login,tel.symbol,tel.currency]);
const stages=[1,2,3].map((stage)=>({position_identifier:907,stage,
 plan_id:'tp-plan-'+String(stage).repeat(32),state:'prepared_only'}));
const view={account_matches:true,context,
 settings:{target:'demo',login:17,server:'D',currency:'USD'},
 partial_exit_stage_plans:{schema:'partial-exit-stage-plans-v1',
 context,target:'demo',read_only:true,orders_enabled:false,
 stage_execution_supported:false,rows:stages},
 partial_exit_price_proposals:{schema:'partial-exit-prices-v1',
 status:'demo_operator_price_proposals_only',
 context,target:'demo',read_only:true,orders_enabled:false,rows:[]}};
const opts={status,source:view,disabled:false};
let rendered=render(opts);
assert(shown(rendered).includes('حساب Demo'));
assert(shown(rendered).includes('لا تُرسل أوامر'));
assert(!collect(rendered).some(x=>x.type==='button'&&!x.props.disabled),
 'Must not save until a position/three prices are selected');
selected='907';inputs=['2001','2002','2003'];dirty=true;
rendered=render(opts);
assert(collect(rendered).some(x=>x.type==='button'&&!x.props.disabled),
 'Exactly staged, broker-tick aligned, increasing prices can be saved');
assert(collect(rendered).filter(x=>x.type==='input').length===3);
inputs=['2001','2001','2003'];
assert(!collect(render(opts)).some(x=>x.type==='button'&&!x.props.disabled),
 'Decreasing/nonmonotone BUY prices must never be saved');
inputs=['2001.005','2002','2003'];
assert(!collect(render(opts)).some(x=>x.type==='button'&&!x.props.disabled),
 'Off-grid broker price must disable save');
inputs=['2001','2002','2003'];
const fake={...view,context:'other-account'};
assert(!collect(render({...opts,source:fake})).some(x=>x.type==='button'&&!x.props.disabled));
assert(!collect(render({...opts,status:{...status,requested_run:true}}))
 .some(x=>x.type==='button'&&!x.props.disabled));
assert(!collect(render({...opts,status:{...status,telemetry:{...tel,demo:false,account_trade_mode:2}}}))
 .some(x=>x.type==='button'&&!x.props.disabled));
const liveAccount={...tel,demo:false,account_trade_mode:2,server:'RealBroker'};
const liveContext=JSON.stringify([liveAccount.server,liveAccount.login,
 liveAccount.symbol,liveAccount.currency]);
const liveView={...view,context:liveContext,
 settings:{...view.settings,target:'live',server:liveAccount.server},
 partial_exit_stage_plans:{...view.partial_exit_stage_plans,
  context:liveContext,target:'live'},
 partial_exit_price_proposals:{...view.partial_exit_price_proposals,
  context:liveContext,target:'live',status:'operator_price_proposals_only'}};
const liveOpts={status:{...status,telemetry:liveAccount},source:liveView,disabled:false};
assert(collect(render(liveOpts)).some(x=>x.type==='button'&&!x.props.disabled),
 'A matching user-selected Live account must be able to save immutable TP plans');
assert(collect(render(liveOpts)).filter(x=>typeof x==='string').join(' ').includes('حساب Live'));
const foreignLive={...liveView,partial_exit_stage_plans:{...liveView.partial_exit_stage_plans,
 target:'demo'}};
assert(!collect(render({...liveOpts,source:foreignLive}))
 .some(x=>x.type==='button'&&!x.props.disabled),
 'Live account must not adopt a Demo stage plan');
assert(!collect(render({...liveOpts,status:{...status,
 telemetry:{...liveAccount,account_trade_mode:0}}}))
 .some(x=>x.type==='button'&&!x.props.disabled),
 'Invalid MT5 trade mode must prevent an account switch');
const changed={...view,partial_exit_stage_plans:{...view.partial_exit_stage_plans,
 rows:stages.map(x=>({...x,state:'review_required'}))}};
assert(!collect(render({...opts,source:changed})).some(x=>x.type==='button'&&!x.props.disabled));
const stored={...view,partial_exit_price_proposals:{...view.partial_exit_price_proposals,
 rows:[{position_identifier:907,prices:[2001,2002,2003]}]}};
assert(!collect(render({...opts,source:stored})).some(x=>x.type==='button'&&!x.props.disabled));
const savedTree=render({...opts,source:stored});
assert(!collect(savedTree).some(x=>x.type==='input'),
 'Saved immutable prices cannot be edited with a second set of input controls');
assert(!collect(savedTree).some(x=>x.type==='button'&&!x.props.disabled),
 'An immutable saved plan must never allow a second write');
const textOf=(tree)=>collect(tree).filter(x=>typeof x==='string').join(' ');
assert(textOf(savedTree).includes('بيانات خطة TP غير موثوقة'),
 'An unknown or legacy price capture must not be described as verified saved prices');
assert(!textOf(savedTree).includes('2001 / 2002 / 2003'),
 'Never echo unverified stored prices as trusted executable levels');
const verified={...view,partial_exit_price_proposals:{...view.partial_exit_price_proposals,
 rows:[{position_identifier:907,prices:[2001,2002,2003],
  state:'price_levels_saved_no_execution'}]}};
const verifiedTree=render({...opts,source:verified});
assert(textOf(verifiedTree).includes('محفوظ للمركز'),
 'A valid, Core-verified immutable research price proposal should remain visible');
assert(textOf(verifiedTree).includes('2001 / 2002 / 2003'));
const stale={...view,partial_exit_price_proposals:{...view.partial_exit_price_proposals,
 rows:[{position_identifier:907,prices:[2001,2002,2003],
  state:'quote_stale_no_trigger'}]}};
assert(textOf(render({...opts,source:stale})).includes('محفوظ للمركز'),
 'A stale quote suppresses crossing, not the integrity of stored prices');
const corrupt={...view,partial_exit_price_proposals:{...view.partial_exit_price_proposals,
 rows:[{position_identifier:907,prices:[2001,2002,2003],state:'invalid_record'}]}};
assert(textOf(render({...opts,source:corrupt})).includes('بيانات خطة TP غير موثوقة'));
assert(!collect(render({...opts,source:corrupt})).some(x=>x.type==='button'&&!x.props.disabled));
const eligibleSaved={...verified,
 partial_exit_stage_consent:{schema:'tp-stage-operator-consent-v1',
  status:'account_scoped_operator_consent_not_execution',purpose:'preparation_only',context,target:'demo',
  read_only:true,automatic_tp_execution_enabled:false,orders_created:0,rows:[]}};
const sealedCore={...eligibleSaved,partial_exit_stage_sealed_core:{
 schema:'tp-stage-sealed-core-intents-v1',status:'account_scoped_prepared_not_sent',
 context,target:'demo',read_only:true,orders_enabled:false,broker_commands_created:0,
 rows:[{position_identifier:907,physical_ticket:87,stage:1,plan_id:stages[0].plan_id,
  state:'sealed_unarmed_not_dispatched',broker_command_id:null,
  gateway_send_id:null,broker_order_ticket:null}]}};
const sealedTree=render({...opts,source:sealedCore});
assert(textOf(sealedTree).includes('سجل هوية TP1 داخل Core'));
assert(textOf(sealedTree).includes('لا يوجد أمر مرسل إلى MT5'));
assert(!textOf(render({...opts,source:{...sealedCore,
 partial_exit_stage_sealed_core:{...sealedCore.partial_exit_stage_sealed_core,target:'live'}}}))
 .includes('سجل هوية TP1 داخل Core'),'A foreign-account seal must stay hidden');
const expiredTree=render({...opts,source:{...sealedCore,
 partial_exit_stage_sealed_core:{...sealedCore.partial_exit_stage_sealed_core,
 rows:[{...sealedCore.partial_exit_stage_sealed_core.rows[0],
 state:'historical_seal_expired_no_send'}]}}});
assert(textOf(expiredTree).includes('سجل TP1 تاريخي'));
assert(!collect(expiredTree).some(x=>x.type==='button'&&
 textOf(x).includes('إرسال TP1')),'Sealing an immutable TP1 ID must never create a send action');
const consentTree=render({...opts,source:eligibleSaved});
assert(textOf(consentTree).includes('تسجيل موافقة هذه الخطة للتحضير المستقبلي'));
assert(!textOf(consentTree).includes('تم تنفيذ TP'),
 'Operator intent is not a broker fill or queued order');
const approvedConsent={...eligibleSaved,partial_exit_stage_consent:{
 ...eligibleSaved.partial_exit_stage_consent,
 rows:[{position_identifier:907,position_ticket:87,
   plan_ids:stages.map(x=>x.plan_id),purpose:'preparation_only',read_only:true,
   orders_enabled:false,broker_command_id:null,
   operator_approved:true,state:'operator_approved_not_dispatched'}]}};
assert(textOf(render({...opts,source:approvedConsent})).includes('إلغاء الموافقة المحفوظة'));
const tradeSource={...approvedConsent,partial_exit_trade_approval:{
 schema:'tp1-trade-approval-v1',status:'account_scoped_trade_preference_not_execution',
 context,target:'demo',read_only:true,tp1_execution_enabled:false,
 broker_commands_created:0,rows:[]}};
const seenTrade=[];
const tradeTree=render({...opts,source:tradeSource,
 onTradeApproval:async(...args)=>{seenTrade.push(args);return true;}});
assert(textOf(tradeTree).includes('تفويض منفصل لتداول TP1 مستقبلاً'));
assert(textOf(tradeTree).includes('التلقائي ما زال مقفلاً في Core'));
const createTrade=collect(tradeTree).find(x=>x.type==='button'&&
 textOf(x).includes('تسجيل تفويض TP1 المستقبلي'));
assert(createTrade&&!createTrade.props.disabled,
 'A verified preparation approval may explicitly create a future-trade preference');
createTrade.props.onClick();
assert.equal(seenTrade.length,1);
assert.equal(seenTrade[0][3],true);
const tradeActiveSource={...tradeSource,partial_exit_trade_approval:{
 ...tradeSource.partial_exit_trade_approval,
 rows:[{position_identifier:907,physical_ticket:87,
 plan_ids:stages.map(r=>r.plan_id),purpose:'tp1_future_trading_preference',
 read_only:true,tp1_execution_enabled:false,orders_enabled:false,
 broker_command_id:null,trade_preference_current:true,
 state:'trade_preference_current_sender_locked'}]}};
const activeTradeTree=render({...opts,source:tradeActiveSource,
 onTradeApproval:async(...args)=>{seenTrade.push(args);return true;}});
assert(textOf(activeTradeTree).includes('الإرسال معطّل'));
const revokeTrade=collect(activeTradeTree).find(x=>x.type==='button'&&
 textOf(x).includes('إلغاء تفويض تداول TP1 المحفوظ'));
assert(revokeTrade&&!revokeTrade.props.disabled);
revokeTrade.props.onClick();
assert.equal(seenTrade[1][3],false,'UI revoke must never reauthorize an old TP1 record');
const tradeStale={...tradeActiveSource,partial_exit_trade_approval:{
 ...tradeActiveSource.partial_exit_trade_approval,
 rows:[{...tradeActiveSource.partial_exit_trade_approval.rows[0],
 trade_preference_current:false,state:'trade_preference_revision_mismatch'}]}};
const staleTradeTree=render({...opts,source:tradeStale});
assert(textOf(staleTradeTree).includes('ألغِه وراجع الإعدادات'));
assert(!collect(staleTradeTree).some(x=>x.type==='button'&&
 textOf(x).includes('تسجيل تفويض TP1 المستقبلي')),
 'Stale prior execution preference must only be revocable');
const lostTrade=render({...opts,status:{...status,telemetry:{...tel,positions:[]}},
 source:tradeStale});
assert(/إلغاء تفويض TP1 للمركز\s+907/.test(textOf(lostTrade)),
 'A missing position must still have a scoped trade-preference revoke control');
const wrongTradeAccount={...tradeSource,partial_exit_trade_approval:{
 ...tradeSource.partial_exit_trade_approval,target:'live'}};
assert(!textOf(render({...opts,source:wrongTradeAccount}))
 .includes('تسجيل تفويض TP1 المستقبلي'),
 'Trade opt-in cannot borrow another account target');
const noPreApproval={...tradeSource,partial_exit_stage_consent:{
 ...tradeSource.partial_exit_stage_consent,rows:[]}};
const blockedTrade=render({...opts,source:noPreApproval});
const disabledTrade=collect(blockedTrade).find(x=>x.type==='button'&&
 textOf(x).includes('تسجيل تفويض TP1 المستقبلي'));
assert(disabledTrade&&disabledTrade.props.disabled,
 'Trade preference requires a verified operator preparation consent');
const invalidRevision={...approvedConsent,partial_exit_stage_consent:{
 ...approvedConsent.partial_exit_stage_consent,
 rows:[{...approvedConsent.partial_exit_stage_consent.rows[0],
  operator_approved:false,state:'operator_approval_revision_mismatch'}]}};
const seen=[];
const staleTree=render({...opts,source:invalidRevision,onConsent:async(...args)=>{seen.push(args);return true;}});
assert(textOf(staleTree).includes('تغيّر المركز/إعدادات التنفيذ'));
assert(!textOf(staleTree).includes('موافقة بحثية مُثبتة للحساب المحدد'));
const revoke=collect(staleTree).find(x=>x.type==='button'&&
 textOf(x).includes('إلغاء الموافقة القديمة غير الصالحة'));
assert(revoke&&!revoke.props.disabled,'Stale matching consent must be revocable');
revoke.props.onClick();
assert.equal(seen.length,1);
assert.equal(seen[0][3],false,'A stale consent must NEVER be re-armed from UI');
const lostPosition=render({...opts,status:{...status,telemetry:{...tel,positions:[]}},
 source:invalidRevision,onConsent:async(...args)=>{seen.push(args);return true;}});
assert(/إلغاء موافقة المركز\s+907/.test(textOf(lostPosition)),
 'Even a missing physical position needs an account-scoped revoke path');
const wrongStoredIds={...invalidRevision,partial_exit_stage_consent:{
 ...invalidRevision.partial_exit_stage_consent,
 rows:[{...invalidRevision.partial_exit_stage_consent.rows[0],
  plan_ids:['forged','forged','forged']}]}};
assert(!collect(render({...opts,source:wrongStoredIds})).some(x=>x.type==='button'&&!x.props.disabled),
 'Malformed historical plan IDs must never authorize a write');

const borrowedConsent={...eligibleSaved,partial_exit_stage_consent:{
 ...eligibleSaved.partial_exit_stage_consent,target:'live'}};
assert(!textOf(render({...opts,source:borrowedConsent})).includes('تسجيل موافقة هذه الخطة للتحضير المستقبلي'),
 'Demo controls cannot adopt Live consent');
const bridge=read('src/bridge.ts');
assert(bridge.includes("invoke('save_partial_exit_prices'"));
const tauri=fs.readFileSync(path.join(root,'../tauri/src/bridge.rs'),'utf8');
assert(tauri.includes('fn save_partial_exit_prices('));
assert(tauri.includes('"/v1/execution/partial-exit-prices"'));
assert(tauri.includes('update_position_protection, save_partial_exit_prices,'));
assert(tauri.includes('fn update_partial_exit_trade_approval('));
assert(tauri.includes('"/v1/execution/partial-exit-trade-approval"'));
assert(bridge.includes("invoke('update_partial_exit_trade_approval'"));
const core=fs.readFileSync(path.join(root,'../../crates/runtime/src/execution.rs'),'utf8');
assert(core.includes('path=="/v1/execution/partial-exit-prices"'));
assert(core.includes('path=="/v1/execution/partial-exit-trade-approval"'));
assert(core.includes('!status.requested_run && !self.pending(db)?'));
const prices=fs.readFileSync(path.join(root,'../../crates/runtime/src/partial_exit_prices.rs'),'utf8');
assert(!prices.includes('order_send('));
assert(prices.includes('orders_enabled":false'));
console.log('PASS: scoped and STOP-only DEMO TP price capture, stale/read-only/frozen controls');
