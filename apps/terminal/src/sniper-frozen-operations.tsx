import type {Status} from './types';

const obj=(v:unknown):Record<string,unknown>=>
 v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Record<string,unknown>:{};
const count=(v:unknown)=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=0?String(v):'—';
export function eligibleFrozenSniper(status:Status|null,page:unknown):boolean{
 const t=status?.telemetry,config=obj(obj(page).config),trading=obj(config.trading);
 return !!(status?.gateway_fresh&&t?.connected&&t.demo&&t.account_trade_mode===0
  &&Number.isSafeInteger(t.login)&&t.login>0&&t.server&&t.symbol&&t.currency
  &&trading.mode==='sniper'&&trading.sniper_brain==='learner');
}
export function SniperFrozenOperations({status,page,report,preview,disabled,onRefresh,onAction}:{
 status:Status|null;page:unknown;report:unknown;preview:boolean;disabled:boolean;
 onRefresh:()=>void;onAction:(action:'freeze'|'stop')=>Promise<void>;
}){
 const t=status?.telemetry,view=obj(report);
 const forward=obj(view.forward_direction_audit);
 const training=obj(view.training_checkpoint);
 const proof=obj(view.research_artifact);
 const history=obj(view.research_artifact_history);
 const journal=obj(view.prospective_event_journal);
 const lastArchivedJournal=obj(view.last_archived_forward_journal);
 const holdout=obj(view.chronological_holdout_diagnostic);
 const recoveredHoldout=obj(view.last_archived_holdout_diagnostic);
 const costEvidence=obj(view.preregistered_cost_evidence);
 const recoveredCosts=obj(view.last_archived_preregistered_costs);
 const booked=obj(view.demo_booked_execution_audit);
 const recoveredBooked=obj(view.last_archived_demo_booked_evidence);
 const fixedCostScore=obj(costEvidence.holdout);
 const protocol=obj(proof.preregistered_forward_protocol);
 const holdoutScore=obj(holdout.holdout);
 const eligible=!preview&&eligibleFrozenSniper(status,page);
 const expected=t?JSON.stringify([t.server,t.login,t.symbol,t.currency]):'';
 const statusValue=view.status;
 const scoped=eligible&&view.context===expected&&view.schema==='p1k3-sniper-frozen-tick-v1'
    &&view.mode==='sniper'&&view.sniper_brain==='learner'
    &&view.no_broker_orders===true&&view.independent_oos===false
    &&view.paper_pnl===null&&view.broker_pnl===null
    &&(statusValue==='active'||statusValue==='stopped');
 const active=scoped&&statusValue==='active';
 const verifiedArtifact=scoped&&proof.schema==='p1k5d-sniper-freeze-artifact-v1'
   &&proof.integrity==='verified'&&proof.id===view.research_artifact_id
   &&proof.sha256===view.research_artifact_sha256
   &&proof.independent_oos===false&&proof.broker_pnl===null
   &&proof.no_broker_orders===true;
 const archive=eligible&&history.schema==='p1k5d-research-artifact-history-v1'
   &&history.account_context===expected&&history.no_broker_orders===true
   &&Array.isArray(history.artifacts)
    ?(history.artifacts as unknown[]).slice(0,8).map(obj):[];
 const trainingVerified=scoped&&training.schema==='p1k5c-sniper-learner-training-cutoff-v1'
   &&training.status==='captured'&&training.source==='deduplicated_complete_mt5_demo_broker_samples'
   &&training.independent_oos===false&&training.broker_pnl===null
   &&typeof training.cutoff_ms==='number'
   &&training.broker_training_samples===training.cached_policy_samples
   &&training.broker_target_samples===training.cached_target_samples;
 const journalVerified=scoped&&verifiedArtifact
   &&journal.schema==='p1k5e-forward-research-journal-v1'
   &&journal.artifact_id===view.research_artifact_id
   &&journal.integrity==='verified_local'
   &&journal.complete_trace===false&&journal.historical_oos===false
   &&journal.broker_pnl===null;
 const archivedJournalVerified=eligible
   &&history.schema==='p1k5d-research-artifact-history-v1'
   &&history.account_context===expected
   &&archive.length>0&&archive[0].integrity==='verified'
   &&lastArchivedJournal.schema==='p1k5e-forward-research-journal-v1'
   &&lastArchivedJournal.artifact_id===archive[0].id
   &&lastArchivedJournal.integrity==='verified_local'
   &&lastArchivedJournal.complete_trace===false
   &&lastArchivedJournal.historical_oos===false
   &&lastArchivedJournal.broker_pnl===null;
 const holdoutVerified=journalVerified
   &&holdout.schema==='p1k5f-chronological-prospective-diagnostic-v1'
   &&holdout.status==='descriptive_holdout_only'
   &&holdout.independent_oos===false&&holdout.broker_pnl===null
   &&holdout.strategy_win_rate===null&&holdout.automatic_promotion===false;
 const recoveredHoldoutVerified=archivedJournalVerified
   &&recoveredHoldout.schema==='p1k5f-chronological-prospective-diagnostic-v1'
   &&recoveredHoldout.status==='descriptive_holdout_only'
   &&recoveredHoldout.independent_oos===false
   &&recoveredHoldout.broker_pnl===null;
 const percent=(v:unknown)=>typeof v==='number'&&Number.isFinite(v)&&v>=0&&v<=1
   ?`${(v*100).toFixed(1)}%`:'—';
 const costVerified=journalVerified
   &&costEvidence.schema==='p1k5g-preregistered-cost-evidence-v1'
   &&costEvidence.status==='descriptive_quote_costs_only'
   &&costEvidence.independent_oos===false&&costEvidence.broker_pnl===null
   &&costEvidence.strategy_win_rate===null&&costEvidence.broker_fills_observed===false
   &&costEvidence.automatic_promotion===false
   &&protocol.schema==='p1k5g-predeclared-prospective-v1'
   &&protocol.registered_at_freeze_ms===costEvidence.registered_at_freeze_ms
   &&protocol.boundary_ms===costEvidence.boundary_ms;
 const archiveCostVerified=archivedJournalVerified
   &&recoveredCosts.schema==='p1k5g-preregistered-cost-evidence-v1'
   &&recoveredCosts.status==='descriptive_quote_costs_only'
   &&recoveredCosts.independent_oos===false&&recoveredCosts.broker_pnl===null;
 const priceMoney=(v:unknown)=>typeof v==='number'&&Number.isFinite(v)
   ?v.toFixed(2):'—';
 const bookedVerified=verifiedArtifact
   &&booked.schema==='p1k5h-demo-booked-trade-audit-v1'
   &&booked.status==='separate_broker_cohort_observed'
   &&booked.source==='reconciled_mt5_demo_broker_deals'
   &&booked.account_context===expected
   &&booked.frozen_artifact_id===view.research_artifact_id
   &&booked.independent_oos===false
   &&booked.frozen_study_sent_orders===false
   &&booked.matched_with_frozen_markouts===false
   &&booked.comparison_with_five_second_markout_permitted===false
   &&booked.booked_costs_reconcile===true
   &&booked.local_broker_deal_receipts_reconciled===true
   &&booked.broker_connection_tested_here===false
   &&booked.real_account_data===false
   &&booked.strategy_win_rate===null;
 const recoveredBookedVerified=archivedJournalVerified
   &&recoveredBooked.schema==='p1k5h-demo-booked-trade-audit-v1'
   &&recoveredBooked.status==='separate_broker_cohort_observed'
   &&recoveredBooked.account_context===expected
   &&recoveredBooked.frozen_artifact_id===archive[0].id
   &&recoveredBooked.matched_with_frozen_markouts===false
   &&recoveredBooked.local_broker_deal_receipts_reconciled===true
   &&recoveredBooked.real_account_data===false;
 const forwardScoped=scoped&&forward.schema==='p1k5b-forward-direction-v1'
   &&forward.independent_oos===false&&forward.broker_fills_observed===false
   &&forward.net_pnl===null&&forward.profitability_validated===false;
 const fraction=forward.observed_direction_fraction;
 const fractionLabel=forwardScoped&&typeof fraction==='number'&&Number.isFinite(fraction)
   &&fraction>=0&&fraction<=1?`${(fraction*100).toFixed(1)}%`:'—';
 const stopped=status?.requested_run===false;
 const ready=eligible&&stopped&&!disabled;
 const canFreeze=ready&&(!scoped||!active);
 const canStop=ready&&active;
 return <section className="operator-card" aria-label="Frozen Sniper Learner forward Tick decisions">
  <h3>Sniper Learner — مقارنة Tick المستقبلية P1-K3</h3>
  <p>تجميد نسخة من نموذج Learner على Demo، ثم مقارنة إشاراتها الخام مع النموذج
    المتكيّف على نفس أسعار Tick المقبولة بعد التجميد. لا نتائج أرباح أو معاملات MT5 إضافية.</p>
  {!scoped?<p role="status" className="safe-note">
   {eligible?'لا توجد تجربة مجمّدة مؤكدة لهذا الحساب؛ هذا ليس تقرير ربحية.':
    'يتطلب حساب Demo حديثاً ووضع Sniper مع العقل Learner. Classic خارج هذه الدراسة.'}
  </p>:<div className="operator-grid">
   <div>حالة التجربة <strong>{active?'نشطة':'متوقفة'}</strong></div>
   <div>تيكات مستقبلية مقبولة <strong>{count(view.accepted_prospective_pulses)}</strong></div>
   <div>قرارات قابلة للمقارنة <strong>{count(view.comparable_future_tick_decisions)}</strong></div>
   <div>قرارات متطابقة <strong>{count(view.exact_decisions)}</strong></div>
   <div>اختلاف جاهزية <strong>{count(view.readiness_disagreements)}</strong></div>
   <div>اختلاف اتجاه <strong>{count(view.side_disagreements)}</strong></div>
   <div>اختلاف هدف/وقف <strong>{count(view.target_or_stop_disagreements)}</strong></div>
   <div>فجوات مصادر مستبعدة <strong>{count(view.excluded_source_switch)}</strong></div>
   <div>فواصل سعرية مستبعدة <strong>{count(view.excluded_broker_clock_gap)}</strong></div>
   <div>فقد طابور Native مستبعد <strong>{count(view.excluded_native_queue_counter_change)}</strong></div>
   <div>لقطات MT5 خارج Pulse (استبعاد) <strong>{count(view.unpaired_reconciled_quote_changes)}</strong></div>
   <div>تغيّر سعر بنفس ملي ثانية الوسيط (مستبعد) <strong>{count(view.unpaired_same_clock_price_changes)}</strong></div>
  </div>}
  {eligible&&statusValue==='integrity_failure'&&
   <p role="alert" className="safe-note">فشل تدقيق SHA-256 للبحث المحفوظ؛
    تم حجب نتائج التجربة الحالية. لا تستخدمها لتقييم أو تشغيل الاستراتيجية.</p>}
  {verifiedArtifact&&<div className="feature-zone" aria-label="Immutable local research checkpoint">
   <h4>سجل تجميد Learner المحفوظ P1-K5D</h4>
   <div className="operator-grid">
    <div>سلامة النسخة <strong>SHA-256 VERIFIED</strong></div>
    <div>تاريخ القطع (ms) <strong>{count(proof.cutoff_ms)}</strong></div>
    <div>معرّف النسخة <strong>{String(proof.id)}</strong></div>
    <div>بصمة نموذج الأوزان <strong>{String(proof.model_scores_sha256).slice(0,16)}…</strong></div>
   </div>
   <p className="safe-note">الأوزان ونقطة التجميد محفوظة في SQLite ويمكن
    التحقق منها بعد إعادة تشغيل Core. البصمة المحلية ليست توقيعاً خارجياً
    ولا شهادة OOS أو نتيجة تداول حي.</p>
  </div>}
  {archive.length>0&&<div className="feature-zone" aria-label="Archived research checkpoints">
   <h4>آخر سجلات التجميد لهذا الحساب (تبقى بعد إعادة تشغيل Core)</h4>
   <div className="operator-grid">
    {archive.map((item,index)=><div key={String(item.id||index)}>
      نسخة {index+1} <strong>{item.integrity==='verified'?'سلامة مؤكدة محلياً':'سلامة فاشلة / غير معتمدة'}</strong>
      <span> {count(item.cutoff_ms)}</span>
    </div>)}
   </div>
  </div>}
  {journalVerified&&<div className="feature-zone" aria-label="Sniper persisted future event journal">
   <h4>سجل أحداث المستقبل المحفوظ P1-K5E</h4>
   <div className="operator-grid">
    <div>أحداث محفوظة <strong>{count(journal.recorded_events)}</strong></div>
    <div>نتائج الاتجاه المكتملة <strong>{count(journal.resolved_labels)}</strong></div>
    <div>أحداث مستبعدة بسبب انقطاع البيانات <strong>{count(journal.censored)}</strong></div>
    <div>أحداث بعد لم تُحفظ <strong>{count(forward.unpersisted_research_events)}</strong></div>
    <div>أحداث فُقدت من ذاكرة الانتظار <strong>{count(forward.dropped_unpersisted_events)}</strong></div>
    <div>إخفاقات الحفظ <strong>{count(view.research_event_write_failures)}</strong></div>
   </div>
   <p className="safe-note">الصفوف المحفوظة تحمل تسلسلاً وبصمة سلامة؛
    لا نستنتج منها وجود سجل كامل لجميع Tick أو نتائج OOS مستقلة.
    آخر أحداث الذاكرة قد تضيع عند تعطل Core قبل التسوية التالية.</p>
  </div>}
  {archivedJournalVerified&&!scoped&&<div className="feature-zone"
    aria-label="Recovered research journal after Core restart">
   <h4>آخر سجل بحثي محفوظ بعد إعادة تشغيل Core</h4>
   <div className="operator-grid">
    <div>أحداث محفوظة <strong>{count(lastArchivedJournal.recorded_events)}</strong></div>
    <div>نتائج مستقبلية موثّقة محلياً <strong>{count(lastArchivedJournal.resolved_labels)}</strong></div>
    <div>نتائج مستبعدة <strong>{count(lastArchivedJournal.censored)}</strong></div>
   </div>
   <p className="safe-note">هذا استرجاع سجلات SQLite للبحث السابق،
    وليس تشغيل دراسة جديدة أو تأكيد اكتمال مسار التيكات.</p>
  </div>}
  {eligible&&(
    journal.integrity==='warning'||lastArchivedJournal.integrity==='warning')&&
   <p role="alert" className="safe-note">تحذير: تعذر التحقق من سلامة
    سجل الأحداث المستقبلية؛ تم حجب عدد النتائج حتى مراجعة البيانات.</p>}
  {bookedVerified&&<div className="feature-zone"
    aria-label="Separate real MT5 Demo booked broker cost audit">
   <h4>صفقات MT5 Demo المنفذة فعلياً — P1-K5H</h4>
   <p className="safe-note">هذه معاملات مُغلقة من سجل الوسيط لنظام التداول
    التنفيذي. <strong>مو صفقات من دراسة Sniper المجمدة</strong>، وما يجوز نقارن أرباحها
    مباشرة مع قياس اتجاه الخمس ثواني.</p>
   <div className="operator-grid">
    <div>مراكز Demo المغلقة <strong>{count(booked.physical_broker_trades)}</strong></div>
    <div>مجموع الأحجام المنفذة (لوت) <strong>{priceMoney(booked.closed_volume_lots_sum)}</strong></div>
    <div>عمولات وسيط مسجلة <strong>{priceMoney(booked.booked_commission_account_currency)}</strong></div>
    <div>Swap مسجل <strong>{priceMoney(booked.booked_swap_account_currency)}</strong></div>
    <div>Fee مسجل <strong>{priceMoney(booked.booked_fee_account_currency)}</strong></div>
    <div>صافي سجل الصفقات المنفصلة (عملة Demo) <strong>
     {priceMoney(booked.net_demo_account_currency)}</strong></div>
   </div>
   <p className="safe-note">طابق Core حساب المراكز والمبالغ مع أوامر الفتح
    وتذاكر Deals الأصلية المخزنة محلياً، حتى مع الإغلاق الجزئي.
    هذا **تدقيق SQLite لسجلات Demo** وليس اختبار اتصال مباشر بوسيط
    MT5 أو نتيجة النموذج المجمد أو دليلاً مستقلاً على الربحية أو قبول REAL.</p>
  </div>}
  {recoveredBookedVerified&&!scoped&&
   <p className="safe-note">أمكن استرجاع سجل صفقات Demo منفصل من قاعدة البيانات.
    هذه ليست تنفيذات الدراسة المجمدة ولا دليل مطابقة مباشر.</p>}
  {verifiedArtifact&&booked.status==='insufficient_broker_evidence'&&
   <p className="safe-note">P1-K5H: لا توجد صفقات MT5 Demo منسوبة فعلياً
    إلى نفس الحساب بعد تجميد الدراسة، أو توجد سجلات غير متسقة
    لا تسمح بإظهار مجموع صافي موثوق.</p>}
  {costVerified&&<div className="feature-zone"
    aria-label="Frozen preregistered quote-only cost sensitivity P1K5G">
   <h4>تكاليف افتراضية محددة قبل الرصد — P1-K5G</h4>
   <div className="operator-grid">
    <div>عينات التقييم بالمستقبل <strong>{count(costEvidence.holdout_labels)}</strong></div>
    <div>عمولة الذهاب والإياب للوت مرجعي <strong>{priceMoney(costEvidence.roundtrip_commission_per_lot)}</strong></div>
    <div>انزلاق مفترض (Tick لكل جهة) <strong>{priceMoney(costEvidence.hypothetical_slippage_ticks_per_side)}</strong></div>
    <div>متوسط تحرك سعر Learner بعد السبريد والتكاليف (لكل لوت مرجعي) <strong>
     {priceMoney(fixedCostScore.mean_modeled_quote_after_cost_per_1_lot)}
    </strong></div>
    <div>متوسط شراء دائم على نفس الفرص وبنفس التكاليف <strong>
     {priceMoney(fixedCostScore.mean_baseline_buy_after_cost_per_1_lot)}
    </strong></div>
    <div>عملة الحساب <strong>{String(costEvidence.account_currency)}</strong></div>
   </div>
   <p className="safe-note">المبلغ تقديري لكل لوت مرجعي واحد اعتماداً على
    Bid/Ask وتكاليف الحساب وقت التجميد؛ لا يمثل لوتك المختار أو ربحاً فعلياً.
    لا يوجد تنفيذ MT5 ولا وقف/هدف أو انزلاق محقق، ولا شهادة OOS.</p>
  </div>}
  {archiveCostVerified&&!scoped&&<p className="safe-note">
   توفرت مقارنة تكاليف وصفية من سجل بحث سابق محفوظ للحساب نفسه.
   هذا تقدير أسعار وليس أرباح صفقات حقيقية.
  </p>}
  {journalVerified&&costEvidence.status==='insufficient_or_unverified'&&
   <p className="safe-note">P1-K5G: بعد ما اكتملت الأيام والعينات
    أو بيانات Bid/Ask وقيمة Tick المطلوبة لحساب تكاليف افتراضية موثوقة.</p>}
  {holdoutVerified&&<div className="feature-zone"
    aria-label="Chronological frozen research diagnostic P1K5F">
   <h4>تقييم زمني وصفي للـLearner — P1-K5F</h4>
   <div className="operator-grid">
    <div>أيام التقييم المحجوزة <strong>{count(holdout.holdout_days)}</strong></div>
    <div>عينات التقييم <strong>{count(holdoutScore.matured_labels)}</strong></div>
    <div>اتجاه Learner الموافق <strong>{percent(holdoutScore.frozen_direction_fraction)}</strong></div>
    <div>خط أساس شراء دائم (نفس الفرص) <strong>{percent(holdoutScore.always_buy_direction_fraction)}</strong></div>
    <div>عينات مستبعدة حول الفاصل <strong>{count(holdout.embargoed_labels)}</strong></div>
   </div>
   <p className="safe-note">آخر يومين من 6 أيام MT5 Demo على الأقل،
    وفاصل 30 دقيقة بين المقارنة والفحص. هذا تحليل اتجاه على فرص Learner
    المنتقاة، وليس اختبار OOS مستقلاً أو Win Rate أو عائداً صافياً بعد التكاليف.</p>
  </div>}
  {journalVerified&&holdout.status==='insufficient_or_unverified'&&
   <p className="safe-note">P1-K5F: لا توجد بعد أيام تداول/عينات مستقبلية كافية
    لإظهار مقارنة زمنية منضبطة. لم تُفترض نتائج مفقودة أو أرباح.</p>}
  {recoveredHoldoutVerified&&!scoped&&
   <p className="safe-note">استرجع Core أيضاً تقريراً زمنياً وصفياً للحساب نفسه
    من سجل البحث السابق؛ لم يُعتبر هذا توثيق OOS مستقلاً.</p>}
  {trainingVerified&&<div className="feature-zone" aria-label="Sniper frozen training ledger checkpoint">
   <h4>توثيق نقطة تجميد بيانات Learner — P1-K5C</h4>
   <div className="operator-grid">
    <div>عينات Demo المكتملة الداخلة في التدريب <strong>{count(training.broker_training_samples)}</strong></div>
    <div>عينات تدريب أهداف pip <strong>{count(training.broker_target_samples)}</strong></div>
    <div>آخر إغلاق MT5 مستخدم (ms) <strong>{count(training.latest_training_closed_ms)}</strong></div>
    <div>وقت قطع التدريب والتجميد (ms) <strong>{count(training.cutoff_ms)}</strong></div>
    <div>صفوف الدفتر التي فُحصت <strong>{count(training.ledger_rows_scanned)}</strong></div>
   </div>
   <p className="safe-note">المحرك طابق عدد العينات المستخدمة في الأوزان مع سجل إغلاقات
   MT5 Demo وقت التجميد؛ هذه بصمة زمنية للبحث وليست توثيقاً مستقلاً OOS
   ولا ربحية متوقعة أو صفقات فعلية من التجربة.</p>
  </div>}
  {forwardScoped&&<div className="feature-zone" aria-label="Frozen Sniper direction-only future observations">
   <h4>تدقيق اتجاه Learner المجمد — أفق 5 ثوانٍ P1-K5B</h4>
   <div className="operator-grid">
    <div>مرشحات مجمدة <strong>{count(forward.frozen_learner_candidates)}</strong></div>
    <div>تقييمات مستقبلية نضجت <strong>{count(forward.resolved_direction_labels)}</strong></div>
    <div>اتجاه مطابق <strong>{count(forward.direction_correct)}</strong></div>
    <div>اتجاه معاكس <strong>{count(forward.direction_opposite)}</strong></div>
    <div>حركة غير كافية <strong>{count(forward.direction_flat)}</strong></div>
    <div>عينات معلقة <strong>{count(forward.pending_unmatured_label)}</strong></div>
    <div>سلاسل غير متصلة مستبعدة <strong>{count(forward.censored_disconnected_segments)}</strong></div>
    <div>نسبة مطابقة اتجاه وصفية (100 عينة على الأقل) <strong>{fractionLabel}</strong></div>
   </div>
   <p className="safe-note">هذا قياس اتجاه منتصف السعر بعد 5 ثوانٍ، مو PnL أو Win Rate
    لصفقات حقيقية. ما يحتسب السبريد التنفيذي أو العمولة أو الانزلاق ولا يؤكد OOS مستقلاً.
    أول 100 نتيجة تعطي وصفاً فقط، بدون تفعيل أو ترقية للعقل.</p>
  </div>}
  <div className="quick-ops">
   <button type="button" disabled={preview||disabled} onClick={onRefresh}>تحديث من Rust Core</button>
   <button type="button" disabled={!canFreeze} onClick={()=>void onAction('freeze')}>تجميد نموذج Sniper Learner</button>
   <button type="button" disabled={!canStop} onClick={()=>void onAction('stop')}>إيقاف المقارنة البحثية</button>
  </div>
  <p className="safe-note">تحتاج STOP لإنشاء/إيقاف التجربة؛ النموذج المجمد يحتفظ بنتائجه
   في ذاكرة Core فقط ويبدأ برصد تيكات مستقبلية مع فترة تهيئة. التوافق ليس معدل نجاح,
   والعينة ليست اختبار OOS مستقلاً أو تصريحاً بالتداول الحقيقي.</p>
 </section>;
}
