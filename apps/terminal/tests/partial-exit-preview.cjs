const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const path=require('node:path');
const ts=require('typescript');
const root=path.resolve(__dirname,'..');
const file=p=>fs.readFileSync(path.join(root,p),'utf8');
const source=file('src/partial-exit-preview.tsx');
const compiled=ts.transpileModule(source,{
 fileName:'partial-exit-preview.tsx',
 compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX},
 reportDiagnostics:true
});
assert.equal((compiled.diagnostics||[]).filter(x=>x.category===ts.DiagnosticCategory.Error).length,0);
const moduleExports={};
const el=(type,props)=>({type,props:props||{}});
vm.runInNewContext(compiled.outputText,{
 exports:moduleExports,require:id=>{
  if(id==='react/jsx-runtime')return {jsx:el,jsxs:el};
  throw Error('Unexpected dependency '+id);
 },Number,Object,Array,JSON,Math,String
});
const render=moduleExports.PartialExitPreview;
function collect(n,all=[]){
 if(n===null||n===undefined)return all;
 if(Array.isArray(n)){n.forEach(x=>collect(x,all));return all}
 if(typeof n==='object'){all.push(n);collect(n.props?.children,all);return all}
 all.push(n);return all;
}
const shown=n=>collect(n).filter(v=>typeof v==='string'||typeof v==='number').join(' ');
const tel={connected:true,demo:true,account_trade_mode:0,login:17,server:'D',currency:'USD',symbol:'XAUUSDm'};
const status={gateway_fresh:true,telemetry:tel};
const context=JSON.stringify([tel.server,tel.login,tel.symbol,tel.currency]);
const view={
 account_matches:true,context,
 settings:{target:'demo',login:17,server:'D',currency:'USD'},
 partial_exit_preview:{
  schema:'partial-exit-volume-preview-v1',
  scope:'owned_positions_current_broker_account_symbol',
  context,target:'demo',status:'read_only_geometry',
  read_only:true,orders_enabled:false,volume_min:0.01,volume_step:0.01,volume_max:100,
  rows:[{position:77,position_volume:0.1,state:'geometry_valid',
    lots:{tp1:0.05,tp2:0.03,tp3:0.02}},
   {position:88,position_volume:0.02,state:'unavailable',
    reason:'too_small_for_three_broker_valid_exits',lots:null}]
 }
};
const rendered=render({status,source:view}),parts=collect(rendered);
assert(parts.some(x=>x.type==='table'),'Renders actual Core preview table');
assert(!parts.some(x=>x.type==='button'),'Read-only cannot send orders');
assert(shown(rendered).includes('0.05') && shown(rendered).includes('0.03'));
assert(shown(rendered).includes('أصغر من ثلاثة إغلاقات'));
assert(shown(rendered).includes('ليست خاصية TP جزئي جاهزة'));
const mismatched=render({status,source:{...view,context:'other-account'}});
assert(!collect(mismatched).some(x=>x.type==='table'),'Do not render cross-account volumes');
const stale=render({status:{...status,gateway_fresh:false},source:view});
assert(!collect(stale).some(x=>x.type==='table'),'Do not render stale account volumes');
const writable=render({status,source:{...view,partial_exit_preview:{...view.partial_exit_preview,orders_enabled:true}}});
assert(!collect(writable).some(x=>x.type==='table'),'A response asserting order permission must fail closed');
const impossible=render({status,source:{...view,partial_exit_preview:{
 ...view.partial_exit_preview,rows:[{position:77,position_volume:0.1,state:'geometry_valid',
 lots:{tp1:0.05,tp2:0.04,tp3:0.04}}]
}}});
assert(!shown(impossible).includes('تقسيم حجم صالح حسابياً فقط'));
const scopedLedger={schema:'partial-exit-draft-ledger-v1',status:'research_drafts_only',
 context,target:'demo',read_only:true,orders_enabled:false,
 rows:[{position_identifier:42,ticket:77,initial_volume:0.1,state:'draft_only'},
       {position_identifier:43,ticket:88,initial_volume:0.1,state:'volume_changed_unverified'},
       {position_identifier:44,ticket:99,initial_volume:0.1,state:'position_missing_unverified'}]};
const withLedger=render({status,source:{...view,partial_exit_draft_ledger:scopedLedger}});
assert(shown(withLedger).includes('سجل مسودات TP المحفوظة'));
assert(shown(withLedger).includes('تغيّر حجم المركز أو هويته'));
assert(shown(withLedger).includes('نتيجة الإغلاق غير مثبتة'));
assert(!collect(withLedger).some(x=>x.type==='button'),'Draft ledger remains read-only');
const foreignLedger=render({status,source:{...view,partial_exit_draft_ledger:{...scopedLedger,context:'other'}}});
assert(!shown(foreignLedger).includes('سجل مسودات TP المحفوظة'),'Never show another account draft');
const forgedLedger=render({status,source:{...view,partial_exit_draft_ledger:{...scopedLedger,orders_enabled:true}}});
assert(!shown(forgedLedger).includes('سجل مسودات TP المحفوظة'),'Never show an executable ledger as read-only');
const stageResponse={schema:'partial-exit-stage-plans-v1',
 status:'draft_identities_only',context,target:'demo',
 read_only:true,orders_enabled:false,stage_execution_supported:false,
 rows:[{position_identifier:42,stage:1,planned_volume:0.05,state:'prepared_only'},
       {position_identifier:42,stage:2,planned_volume:0.03,state:'review_required'}]};
const withStages=render({status,source:{
 ...view,partial_exit_draft_ledger:scopedLedger,partial_exit_stage_plans:stageResponse}});
assert(shown(withStages).includes('هويات مراحل TP المحفوظة'));
assert(shown(withStages).includes('أُلغي اعتماد الخطة'));
assert(!collect(withStages).some(x=>x.type==='button'),'TP stage plans are diagnostics only');
const mismatchedStages=render({status,source:{
 ...view,partial_exit_draft_ledger:scopedLedger,
 partial_exit_stage_plans:{...stageResponse,context:'other'}}});
assert(!shown(mismatchedStages).includes('هويات مراحل TP المحفوظة'));
const executableStages=render({status,source:{
 ...view,partial_exit_draft_ledger:scopedLedger,
 partial_exit_stage_plans:{...stageResponse,stage_execution_supported:true}}});
assert(!shown(executableStages).includes('هويات مراحل TP المحفوظة'));
const audit={schema:'partial-exit-deal-evidence-v1',status:'broker_history_audit_only',
 context,target:'demo',read_only:true,orders_enabled:false,tp_stage_execution_supported:false,
 rows:[{position_identifier:42,evidence:{state:'broker_partial_exit_unattributed',
   broker_closed_lots:0.04,booked_net_account_currency:-0.27,
   tp_stage_attribution_verified:false,stage_completed:false}}]};
const observed=render({status,source:{...view,partial_exit_deal_evidence:audit}});
assert(shown(observed).includes('مراجعة إغلاقات MT5 الموثقة'));
assert(shown(observed).includes('غير منسوب إلى أي هدف TP'));
assert(shown(observed).includes('0.04 لوت مُغلق'));
assert(shown(observed).includes('صافي قيود الوسيط المحفوظة'));
assert(shown(observed).includes('-0.27'));
assert(shown(observed).includes('ليس ربح TP'));
assert(!collect(observed).some(x=>x.type==='button'));
const wrongAudit=render({status,source:{...view,
 partial_exit_deal_evidence:{...audit,context:'other'}}});
assert(!shown(wrongAudit).includes('مراجعة إغلاقات MT5 الموثقة'));
const forgedFill=render({status,source:{...view,
 partial_exit_deal_evidence:{...audit,rows:[{position_identifier:42,evidence:{
 state:'broker_partial_exit_unattributed',broker_closed_lots:0.04,
 tp_stage_attribution_verified:true,stage_completed:true}}]}}});
assert(!shown(forgedFill).includes('صافي قيود الوسيط المحفوظة'),
 'Never show costed trade performance from a forged TP completion');
assert(!shown(forgedFill).includes('0.04 لوت مُغلق'),
 'TP completion asserted without an executable command must never get credit');
const linked={schema:'partial-exit-stage-command-links-v1',
 status:'broker_stage_links_read_only',context,target:'demo',
 read_only:true,orders_enabled:false,stage_execution_supported:false,
 rows:[{position_identifier:907,stage:1,plan_id:'tp-plan-a',
 evidence:{state:'broker_order_deals_exactly_linked',matched_broker_lots:0.05,
          stage_completed:false,orders_enabled:false}}]};
const linkView=render({status,source:{...view,partial_exit_command_links:linked}});
assert(shown(linkView).includes('مطابقة مرحلة TP مع أمر Core'));
assert(shown(linkView).includes('0.05 لوت موثق'));
assert(!collect(linkView).some(x=>x.type==='button'));
const badLinked=render({status,source:{...view,partial_exit_command_links:{...linked,context:'foreign'}}});
assert(!shown(badLinked).includes('مطابقة مرحلة TP مع أمر Core'));
const stagedFalseProof=render({status,source:{...view,partial_exit_command_links:{
 ...linked,rows:[{position_identifier:907,stage:1,plan_id:'tp-plan-a',
 evidence:{state:'broker_order_deals_exactly_linked',matched_broker_lots:0.05,
 stage_completed:true,orders_enabled:true}}]}}});
assert(!shown(stagedFalseProof).includes('0.05 لوت موثق'));
const nextGate={schema:'partial-exit-next-shadow-gate-v1',
 status:'verified_history_research_only',context,target:'demo',
 read_only:true,orders_enabled:false,next_stage_authorized:false,
 breakeven_enabled:false,
 rows:[{position_identifier:907,plan_id:'tp-plan-b',stage:2,
 state:'historical_prerequisites_met_shadow_only',
 historical_condition_only:true,read_only:true,orders_enabled:false,
 next_stage_authorized:false,breakeven_enabled:false,
 expected_close_lots:0.03,expected_remaining_volume:0.02}]};
const nextView=render({status,source:{...view,partial_exit_next_shadow_gate:nextGate}});
assert(shown(nextView).includes('فحص شروط TP2 / TP3'));
assert(shown(nextView).includes('لوت مفترض 0.03'));
assert(!collect(nextView).some(x=>x.type==='button'));
const nextForeign=render({status,source:{...view,
 partial_exit_next_shadow_gate:{...nextGate,context:'foreign'}}});
assert(!shown(nextForeign).includes('فحص شروط TP2 / TP3'));
const forgedNext=render({status,source:{...view,
 partial_exit_next_shadow_gate:{...nextGate,rows:[
 {...nextGate.rows[0],next_stage_authorized:true}]}}});
assert(!shown(forgedNext).includes('لوت مفترض 0.03'));
const shadowIntent={schema:'tp-shadow-close-intents-v1',
 status:'research_close_intents_no_broker_commands',context,target:'demo',
 read_only:true,orders_enabled:false,next_stage_authorized:false,
 breakeven_enabled:false,
 rows:[{position_identifier:907,plan_id:'tp-plan-a',stage:1,
 ticket:87,state:'shadow_close_intent_only',close_lots:0.05,
 expected_remaining_volume:0.05,read_only:true,
 orders_enabled:false,broker_fill_verified:false,
 next_stage_authorized:false,breakeven_enabled:false}]};
const intentView=render({status,source:{...view,partial_exit_shadow_close_intents:shadowIntent}});
assert(shown(intentView).includes('مسودة حجم إغلاق TP1'));
assert(shown(intentView).includes('إغلاق 0.05 / متبقٍ 0.05 لوت'));
assert(!collect(intentView).some(x=>x.type==='button'));
const foreignIntent=render({status,source:{...view,
 partial_exit_shadow_close_intents:{...shadowIntent,context:'foreign'}}});
assert(!shown(foreignIntent).includes('مسودة حجم إغلاق TP1'));
const forgedIntent=render({status,source:{...view,
 partial_exit_shadow_close_intents:{...shadowIntent,
 rows:[{...shadowIntent.rows[0],broker_fill_verified:true}]}}});
assert(!shown(forgedIntent).includes('إغلاق 0.05 / متبقٍ 0.05 لوت'));
const shadow={schema:'partial-exit-shadow-tp1-v1',
 status:'shadow_crossings_no_broker_orders',context,target:'demo',
 read_only:true,orders_enabled:false,next_stage_authorized:false,
 breakeven_enabled:false,
 rows:[{position_identifier:907,stage:1,plan_id:'tp-plan-test',
 state:'historical_shadow_crossing_only',
 source_quote_time_ms:20261103010200,observed_price:2001.2,target_price:2001,
 orders_enabled:false,broker_fill_verified:false,next_stage_authorized:false,
 breakeven_enabled:false}]};
const shadowTree=render({status,source:{...view,partial_exit_shadow_tp1:shadow}});
assert(shown(shadowTree).includes('رصْد عبور TP1'));
assert(shown(shadowTree).includes('عبور سعري بحثي مؤرشف فقط'));
assert(!collect(shadowTree).some(x=>x.type==='button'));
const fakeShadow=render({status,source:{...view,
 partial_exit_shadow_tp1:{...shadow,context:'another-account'}}});
assert(!shown(fakeShadow).includes('رصْد عبور TP1'));
const unsafeShadow=render({status,source:{...view,
 partial_exit_shadow_tp1:{...shadow,rows:[{...shadow.rows[0],broker_fill_verified:true}]}}});
assert(!shown(unsafeShadow).includes('عبور سعري بحثي مؤرشف فقط'));
const settlement={schema:'partial-exit-stage-settlements-v1',
 status:'account_scoped_historical_settlements',context,target:'demo',
 read_only:true,orders_enabled:false,next_stage_authorized:false,
 breakeven_enabled:false,
 rows:[{position_identifier:907,stage:1,plan_id:'tp-plan-test',
 state:'historical_stage_sealed',matched_volume:0.05,broker_order:777,
 next_stage_authorized:false,breakeven_triggered:false}]};
const sealedView=render({status,source:{...view,partial_exit_stage_settlements:settlement}});
assert(shown(sealedView).includes('تسوية TP1/TP2/TP3'));
assert(shown(sealedView).includes('0.05 لوت مُصالح'));
assert(!collect(sealedView).some(x=>x.type==='button'));
const wrongSettlement=render({status,source:{...view,
 partial_exit_stage_settlements:{...settlement,context:'foreign'}}});
assert(!shown(wrongSettlement).includes('تسوية TP1/TP2/TP3'));
const revokedEvidenceView=render({status,source:{...view,
 partial_exit_stage_settlements:{...settlement,
 rows:[{position_identifier:907,stage:1,plan_id:'tp-plan-test',
 state:'sealed_evidence_needs_review',matched_volume:0.05,broker_order:777,
 next_stage_authorized:false,breakeven_triggered:false}]}}});
assert(shown(revokedEvidenceView).includes('سند الوسيط الحالي تغيّر أو فقد'));
assert(!shown(revokedEvidenceView).includes('0.05 لوت مُصالح'));
const unsafeSettlement=render({status,source:{...view,
 partial_exit_stage_settlements:{...settlement,
 rows:[{position_identifier:907,stage:1,plan_id:'tp-plan-test',
 state:'historical_stage_sealed',matched_volume:0.05,broker_order:777,
 next_stage_authorized:true,breakeven_triggered:true}]}}});
assert(!shown(unsafeSettlement).includes('0.05 لوت مُصالح'));
// The user selects Demo/Live: broker TP research evidence must be displayed
// for the same selected Core account, never hidden only because it is Live.
const liveTel={...tel,demo:false,account_trade_mode:2,server:'LiveBroker'};
const liveStatus={...status,telemetry:liveTel};
const liveContext=JSON.stringify([liveTel.server,liveTel.login,liveTel.symbol,liveTel.currency]);
const liveView={...view,context:liveContext,
 settings:{...view.settings,target:'live',server:liveTel.server},
 partial_exit_preview:{...view.partial_exit_preview,context:liveContext,target:'live'},
 partial_exit_shadow_tp1:{...shadow,context:liveContext,target:'live'},
 partial_exit_next_shadow_gate:{...nextGate,context:liveContext,target:'live'},
 partial_exit_shadow_close_intents:{...shadowIntent,context:liveContext,target:'live'},
 partial_exit_stage_settlements:{...settlement,context:liveContext,target:'live'}};
const realEvidence=render({status:liveStatus,source:liveView});
for(const label of ['رصْد عبور TP1','تسوية TP1/TP2/TP3','فحص شروط TP2 / TP3',
 'مسودة حجم إغلاق TP1']){
 assert(shown(realEvidence).includes(label),
  'Live broker account must show verified scoped TP research evidence: '+label);
}
assert(!collect(realEvidence).some(x=>x.type==='button'),
 'Live research TP evidence cannot become an order action');
const foreignReal=render({status:liveStatus,source:{...liveView,
 partial_exit_shadow_tp1:{...shadow,context:liveContext,target:'demo'},
 partial_exit_stage_settlements:{...settlement,context:liveContext,target:'demo'}}});
assert(!shown(foreignReal).includes('رصْد عبور TP1'));
assert(!shown(foreignReal).includes('تسوية TP1/TP2/TP3'));
const wrongMode=render({status:{...liveStatus,
 telemetry:{...liveTel,account_trade_mode:0}},source:liveView});
assert(!collect(wrongMode).some(x=>x.type==='table'),
 'Contradictory live broker trade mode must not show account data');
assert(!shown(wrongMode).includes('مسودة حجم إغلاق TP1'));
const stageCommandDraft={
 schema:'tp-stage-broker-command-drafts-v1',
 status:'broker_command_shapes_not_queued',context:liveContext,target:'live',
 read_only:true,orders_enabled:false,authorized:false,broker_commands_created:0,
 rows:[{stage:1,position_identifier:907,plan_id:'tp-plan-test',
  read_only:true,orders_enabled:false,broker_command_id:null,
  broker_command_draft:{
   schema:'tp-stage-broker-command-shape-v1',kind:'close',
   context:liveContext,target:'live',login:liveTel.login,
   server:liveTel.server,symbol:liveTel.symbol,
   tp_stage:1,tp_stage_plan_id:'tp-plan-test',lots:0.05,
   tp_target_price:2001,expected_position_volume:0.1,
   expected_position_entry:1999.5,expected_broker_tp:0.0}}]};
const stagedCommandView=render({status:liveStatus,source:{
 ...liveView,partial_exit_stage_command_drafts:stageCommandDraft}});
assert(shown(stagedCommandView).includes('تركيب أمر TP داخل Rust Core'));
assert(shown(stagedCommandView).includes('لوت 0.05 · هدف 2,001'),
 'Only exact, identified source-level command drafts may be shown');
assert(!collect(stagedCommandView).some(x=>x.type==='button'),
 'No send button may exist for an unqueued Live stage candidate');
const injectedId=render({status:liveStatus,source:{...liveView,
 partial_exit_stage_command_drafts:{...stageCommandDraft,
 rows:[{...stageCommandDraft.rows[0],
 broker_command_draft:{...stageCommandDraft.rows[0].broker_command_draft,
  id:'fake-command'}}]}}});
assert(!shown(injectedId).includes('لوت 0.05 · هدف 2,001'),
 'Never present a payload with a claimed broker command ID as a read-only draft');
const foreignStageCommand=render({status:liveStatus,source:{...liveView,
 partial_exit_stage_command_drafts:{...stageCommandDraft,target:'demo'}}});
assert(!shown(foreignStageCommand).includes('تركيب أمر TP داخل Rust Core'),
 'A Demo stage draft must not appear in Live account scope');
const core=file('../../crates/runtime/src/execution.rs');
const domain=file('../../crates/domain/src/partial_exit.rs');
const terminal=file('src/terminal.tsx');
assert(core.includes('value["partial_exit_preview"]=if status.gateway_fresh && self.account_matches(status)'));
assert(core.includes('value["partial_exit_draft_ledger"]=if status.gateway_fresh && self.account_matches(status)'));
assert(core.includes('value["partial_exit_stage_plans"]=if status.gateway_fresh && self.account_matches(status)'));
assert(core.includes('value["partial_exit_deal_evidence"]=if status.gateway_fresh && self.account_matches(status)'));
assert(core.includes('value["partial_exit_command_links"]=if status.gateway_fresh && self.account_matches(status)'));
assert(core.includes('value["partial_exit_stage_settlements"]=if status.gateway_fresh && self.account_matches(status)'));
assert(core.includes('value["partial_exit_shadow_tp1"]=if status.gateway_fresh && self.account_matches(status)'));
assert(core.includes('value["partial_exit_shadow_close_intents"]=if status.gateway_fresh && self.account_matches(status)'));
assert(core.includes('value["partial_exit_next_shadow_gate"]=if status.gateway_fresh && self.account_matches(status)'));
const shadowNext=file('../../crates/runtime/src/partial_exit_next_shadow.rs');
assert(shadowNext.includes('historical_prerequisites_met_shadow_only'));
assert(shadowNext.includes('historical_stage_sealed'));
assert(!shadowNext.includes('order_send('));

const shadowIntentCore=file('../../crates/runtime/src/partial_exit_shadow_intent.rs');
assert(shadowIntentCore.includes('partial_exit_shadow_close_intents'));
assert(shadowIntentCore.includes('research_close_intent'));
assert(!shadowIntentCore.includes('order_send('));

const shadowRust=file('../../crates/runtime/src/partial_exit_shadow_trigger.rs');
assert(shadowRust.includes('partial_exit_shadow_tp1'));
assert(shadowRust.includes('source_quote_time_ms'));
assert(!shadowRust.includes('order_send('));

const settleSrc=file('../../crates/runtime/src/partial_exit_progress.rs');
assert(settleSrc.includes('partial_exit_stage_settlements'));
assert(settleSrc.includes('authorizes_next_stage')); 
assert(!settleSrc.includes('order_send('));

const linkRust=file('../../crates/runtime/src/partial_exit_link.rs');
assert(linkRust.includes('broker_order_deals_exactly_linked'));
assert(linkRust.includes('d["order"].as_u64()!=Some(order)'));
assert(!linkRust.includes('order_send('));

const evidenceSrc=file('../../crates/runtime/src/partial_exit_evidence.rs');
assert(evidenceSrc.includes('tp_stage_attribution_verified'));
assert(evidenceSrc.includes('broker_partial_exit_unattributed'));
assert(!evidenceSrc.includes('order_send('));
const stageSource=file('../../crates/runtime/src/partial_exit_stage.rs');
assert(stageSource.includes('PRIMARY KEY(context,target,position_identifier,stage)'));
assert(stageSource.includes('review_required'));
assert(!stageSource.includes('order_send('));
const ledgerSource=file('../../crates/runtime/src/partial_exit_ledger.rs');
assert(ledgerSource.includes('position_missing_unverified'));
assert(ledgerSource.includes('volume_changed_unverified'));
assert(!ledgerSource.includes('order_send('));
assert(domain.includes('pub fn three_target_lots('));
assert(domain.includes('pub fn preview('));
assert(terminal.includes('<PartialExitPreview status={!preview&&feed.broker?status:null} source={portfolioReport}/>'));
console.log('PASS: TP preview fail-closed account binding, source geometry, small lot warnings, and zero order controls');
