import type {Status} from './types';

const object=(v:unknown):Record<string,unknown> =>
 v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Record<string,unknown>:{};
const valid=(v:unknown):v is number=>typeof v==='number'&&Number.isFinite(v)&&v>=0;
const volume=(v:unknown)=>valid(v)?v.toLocaleString('en-US',{maximumFractionDigits:8}):'—';
const reasons:Record<string,string>={
 broker_volume_rules_unavailable:'حد اللوت أو خطوته أو سقفه غير متاح من الوسيط',
 missing_or_invalid_broker_volume_rules:'قيم حجم العقد عند الوسيط غير صالحة',
 volume_exceeds_reported_broker_range:'حجم المركز خارج النطاق الذي أثبته الوسيط',
 unverified_minimum_volume_grid:'الحد الأدنى لا يتطابق مع شبكة خطوات حجم الوسيط',
 unverified_maximum_volume_grid:'أقصى حجم لا يتطابق مع شبكة خطوات حجم الوسيط',
 position_volume_off_broker_step:'حجم المركز لا يطابق خطوة الحجم المتاحة',
 position_volume_outside_broker_grid:'حجم المركز خارج شبكة الأحجام الموثقة',
 too_small_for_three_broker_valid_exits:'حجم الصفقة أصغر من ثلاثة إغلاقات بالحد الأدنى',
 untradeable_final_residual:'سيبقى حجم غير قابل للإغلاق وفق قواعد الوسيط',
 volume_unit_overflow:'عدد خطوات الحجم كبير أو غير موثوق'
};
/** Read-only plan from Rust domain: never sends an MT5 close. */
export function PartialExitPreview({status,source}:{status:Status|null;source:unknown}){
 const t=status?.telemetry,view=object(source),settings=object(view.settings);
 const p=object(view.partial_exit_preview);
 const context=t?JSON.stringify([t.server,t.login,t.symbol,t.currency]):'';
 const verified=status?.gateway_fresh===true&&t?.connected===true&&
  t.login>0&&!!t.server&&!!t.symbol&&!!t.currency&&
  t.account_trade_mode===(t.demo?0:2)&&
  view.account_matches===true&&view.context===context&&
  settings.login===t.login&&settings.server===t.server&&
  settings.currency===t.currency&&settings.target===(t.demo?'demo':'live')&&
  p.schema==='partial-exit-volume-preview-v1'&&
  p.scope==='owned_positions_current_broker_account_symbol'&&
  p.context===context&&p.target===(t.demo?'demo':'live')&&
  p.status==='read_only_geometry'&&p.read_only===true&&
  p.orders_enabled===false;
 const rows=verified&&Array.isArray(p.rows)?p.rows.slice(0,25).map(object):[];
 const ledger=object(view.partial_exit_draft_ledger);
 const ledgerBound=verified&&ledger.schema==='partial-exit-draft-ledger-v1'&&
  ledger.status==='research_drafts_only'&&ledger.context===context&&
  ledger.target===(t?.demo?'demo':'live')&&ledger.read_only===true&&
  ledger.orders_enabled===false;
 const drafts=ledgerBound&&Array.isArray(ledger.rows)?ledger.rows.slice(0,25).map(object):[];
 const stages=object(view.partial_exit_stage_plans);
 const stagesBound=ledgerBound&&stages.schema==='partial-exit-stage-plans-v1'&&
  stages.status==='draft_identities_only'&&stages.context===context&&
  stages.target===(t?.demo?'demo':'live')&&stages.read_only===true&&
  stages.orders_enabled===false&&stages.stage_execution_supported===false;
 const stageRows=stagesBound&&Array.isArray(stages.rows)?stages.rows.slice(0,75).map(object):[];
 const dealAudit=object(view.partial_exit_deal_evidence);
 const dealAuditBound=verified&&dealAudit.schema==='partial-exit-deal-evidence-v1'&&
  dealAudit.status==='broker_history_audit_only'&&dealAudit.context===context&&
  dealAudit.target===(t?.demo?'demo':'live')&&dealAudit.read_only===true&&
  dealAudit.orders_enabled===false&&dealAudit.tp_stage_execution_supported===false;
 const dealRows=dealAuditBound&&Array.isArray(dealAudit.rows)?dealAudit.rows.slice(0,25).map(object):[];
 const dealState:Record<string,string>={
  invalid_deal_history:'السجل يتضمن صفقة أو قيمة غير صالحة — لا تعتمد على الحجم',
  incomplete_or_ambiguous_open_history:'صفقات فتح المركز ناقصة أو غير مؤكدة',
  no_broker_exit_deals:'لا يوجد إغلاق موثق ضمن السجل المحفوظ',
  broker_partial_exit_unattributed:'يوجد إغلاق جزئي مثبت من MT5 لكنه غير منسوب إلى أي هدف TP',
  broker_flat_by_deals_unattributed:'حجم الفتح والإغلاق متساويان في سجل MT5؛ لم يُثبت أي هدف TP',
  broker_history_not_synced:'لم يكتمل مزامنة سجل الوسيط لهذا الحساب',
  too_many_deals_to_audit:'عدد سجلات المركز تجاوز حد الفحص',
  invalid_draft_geometry:'خطوة اللوت أو حجم المسودة غير صالح',
  invalid_draft_identity:'هوية مسودة الصفقة الأصلية أو اتجاهها غير مطابقة — لا تعتمد على التاريخ'
 };
 const draftState:Record<string,string>={
  draft_only:'مسودة محفوظة فقط — لم يُرسل أي أمر',
  volume_changed_unverified:'تغيّر حجم المركز أو هويته — يحتاج مراجعة',
  position_missing_unverified:'المركز غير ظاهر — نتيجة الإغلاق غير مثبتة',
  invalid_draft_journal:'سجل الخطة غير صالح — لا تعتمد عليه'
 };
 return <section className="operator-card" aria-label="معاينة أهداف TP الجزئية للوت">
  <h3>معاينة توزيع حجم TP1 / TP2 / TP3</h3>
  <p>تقسيم حسابي للوت فقط وفق بيانات MT5: 50%، 30%، 20%. لا تُنشأ أهداف سعرية ولا تُرسل أوامر إغلاق. هذه ليست خاصية TP جزئي جاهزة للتداول.</p>
  {!verified?<p className="safe-note">المعاينة غير متاحة: يجب التحقق من حساب MT5 والرمز وإعدادات الحجم الحالية لدى Core.</p>:
   <>
    <p className="safe-note">حد اللوت: <b>{volume(p.volume_min)}</b> · خطوة الحجم: <b>{volume(p.volume_step)}</b> · الحد الأعلى: <b>{volume(p.volume_max)}</b></p>
    {rows.length===0?<p className="safe-note">لا توجد مراكز BrainTrade مؤهلة للمعاينة في الرمز الحالي. لا تُعرض مراكز الحساب اليدوية أو الرموز الأخرى.</p>:
     <div className="table-scroll"><table><thead><tr>
      {['مركز MT5','الحجم','TP1 (50%)','TP2 (30%)','TP3 (المتبقي)','التحقق'].map(label=><th key={label}>{label}</th>)}
     </tr></thead><tbody>
      {rows.map((r,i)=>{
       const l=object(r.lots);
       const amounts=[l.tp1,l.tp2,l.tp3];
       const consistent=r.state==='geometry_valid'&&
        valid(r.position_volume)&&valid(p.volume_step)&&p.volume_step>0&&
        amounts.every(valid)&&
        Math.abs(amounts.reduce<number>((a,b)=>a+(b as number),0)-r.position_volume)<=(p.volume_step as number)*1e-6;
       return <tr key={String(r.position)+'-'+String(i)}>
        <td>{Number.isSafeInteger(r.position)?String(r.position):'—'}</td>
        <td>{volume(r.position_volume)}</td>
        <td>{consistent?volume(l.tp1):'—'}</td>
        <td>{consistent?volume(l.tp2):'—'}</td>
        <td>{consistent?volume(l.tp3):'—'}</td>
        <td>{consistent?'تقسيم حجم صالح حسابياً فقط':reasons[String(r.reason)]||'لم يتم توثيق صلاحية التقسيم'}</td>
       </tr>;
      })}
     </tbody></table></div>}
   {ledgerBound?<div className="execution-binding-summary" aria-label="سجل مسودات TP الدائم">
     <strong>سجل مسودات TP المحفوظة في Rust Core</strong>
     <p className="safe-note">هذا سجل تدريبي دائم للخطط الحسابية فقط. لا يُرسل أوامر ولا يعتبر اختفاء المركز أو انخفاض حجمه دليلاً على تنفيذ TP.</p>
     {drafts.length===0?<p>لا توجد مسودة محفوظة للحساب والرمز الحالي.</p>:
      drafts.map((d,i)=><div className="brain-control-row" key={String(d.position_identifier)+'-'+i}>
       <div><b>مركز #{Number.isSafeInteger(d.ticket)?String(d.ticket):'—'}</b>
        <small style={{display:'block',marginTop:4}}>{draftState[String(d.state)]||'حالة غير موثقة — راجع Core'}</small>
       </div>
       <span>الحجم الأصلي {volume(d.initial_volume)} لوت</span>
      </div>)}
    </div>:null}
   {dealAuditBound?<details className="operator-advanced" aria-label="دليل الإغلاقات الفعلية من MT5">
     <summary>مراجعة إغلاقات MT5 الموثقة — لا تثبت تحقق TP</summary>
     <p className="safe-note">المبالغ هنا من سجل Broker Deals المحفوظ في Core، وليست أرقام الشادو أو المحاكاة. إغلاق المركز بحد ذاته لا يثبت أنه نُفّذ ضمن TP1/2/3؛ لا توجد أي مرحلة مذكورة كمنفذة.</p>
     {dealRows.length===0?<p>لا توجد مراكز مخططة يمكن فحص سجل إغلاقاتها.</p>:
      dealRows.map((r,i)=>{const e=object(r.evidence);
       const usable=e.tp_stage_attribution_verified===false&&e.stage_completed===false;
       return <div className="brain-control-row" key={String(r.position_identifier)+'-'+i}>
        <div><b>مركز {Number.isSafeInteger(r.position_identifier)?String(r.position_identifier):'—'}</b>
         <small style={{display:'block',marginTop:4}}>
          {usable?(dealState[String(e.state)]||'نتيجة السجل غير موثقة'):'نتيجة غير آمنة أو مجهولة — لا يمكن تأكيد تنفيذ TP'}
         </small>
        </div>
        <span>{usable&&valid(e.broker_closed_lots)?volume(e.broker_closed_lots)+' لوت مُغلق في السجل':'غير موثق'}</span>
       </div>;
      })}
     <p className="safe-note">العمولة والسواب والرسوم جزء من سجل الصفقات، لكن إثبات ربح كل هدف يحتاج ارتباطاً منفصلاً بأمر الإغلاق وDeal ID الفعلي.</p>
    </details>:null}
   {stagesBound?<details className="operator-advanced" aria-label="هويات مراحل TP المخطط لها">
     <summary>هويات مراحل TP المحفوظة (عدد {stageRows.length}) — غير قابلة للتنفيذ</summary>
     <p className="safe-note">كل مرحلة لديها هوية ثابتة تمنع تكرار التخطيط عند إعادة التشغيل؛ هذه ليست تذكرة MT5، ولا تعني أن السعر وصل للهدف أو أُغلق أي لوت.</p>
     {stageRows.map((r,i)=><div className="brain-control-row" key={String(r.position_identifier)+'-'+String(r.stage)+'-'+i}>
      <div><b>TP{[1,2,3].includes(Number(r.stage))?String(r.stage):'؟'} — مركز {Number.isSafeInteger(r.position_identifier)?String(r.position_identifier):'—'}</b>
       <small style={{display:'block',marginTop:4}}>{r.state==='prepared_only'?'هوية خطة محفوظة فقط — لم يحدث تنفيذ':
         r.state==='review_required'?'أُلغي اعتماد الخطة بعد تغير بيانات MT5 — يحتاج مراجعة':
         'حالة غير موثقة — لا تعتمد عليها'}</small>
      </div><span>{volume(r.planned_volume)} لوت</span>
     </div>)}
    </details>:null}
   </>}
  <p className="safe-note">حتى إذا صلح تقسيم اللوت، التنفيذ يحتاج مدير إغلاقات جزئية منفصل، ومصالحة حجم الصفقة الفعلي بعد كل هدف، واختبار Hedging/Netting والسبريد والانزلاق وحدود الوسيط. لا تعتبر هذه الشاشة موافقة على LIVE.</p>
 </section>;
}
