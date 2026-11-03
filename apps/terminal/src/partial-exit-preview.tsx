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
 const accountTarget=t?.demo===true?'demo':t?.demo===false?'live':null;
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
 const shadow=object(view.partial_exit_shadow_tp1);
 const shadowBound=verified&&
  shadow.schema==='partial-exit-shadow-tp1-v1'&&
  shadow.status==='shadow_crossings_no_broker_orders'&&
  shadow.context===context&&shadow.target===accountTarget&&
  shadow.read_only===true&&shadow.orders_enabled===false&&
  shadow.next_stage_authorized===false&&shadow.breakeven_enabled===false;
 const shadowRows=shadowBound&&Array.isArray(shadow.rows)?
  shadow.rows.slice(0,50).map(object):[];
 const nextGate=object(view.partial_exit_next_shadow_gate);
 const nextGateBound=verified&&
  nextGate.schema==='partial-exit-next-shadow-gate-v1'&&
  ['verified_history_research_only','no_broker_verified_previous_stage'].includes(String(nextGate.status))&&
  nextGate.context===context&&nextGate.target===accountTarget&&
  nextGate.read_only===true&&nextGate.orders_enabled===false&&
  nextGate.next_stage_authorized===false&&nextGate.breakeven_enabled===false;
 const nextGateRows=nextGateBound&&Array.isArray(nextGate.rows)?
  nextGate.rows.slice(0,25).map(object):[];
 const intent=object(view.partial_exit_shadow_close_intents);
 const intentBound=verified&&
  intent.schema==='tp-shadow-close-intents-v1'&&
  intent.status==='research_close_intents_no_broker_commands'&&
  intent.context===context&&intent.target===accountTarget&&
  intent.read_only===true&&intent.orders_enabled===false&&
  intent.next_stage_authorized===false&&intent.breakeven_enabled===false;
 const intentRows=intentBound&&Array.isArray(intent.rows)?
  intent.rows.slice(0,50).map(object):[];
 const settlements=object(view.partial_exit_stage_settlements);
 const settlementsBound=verified&&
  settlements.schema==='partial-exit-stage-settlements-v1'&&
  settlements.status==='account_scoped_historical_settlements'&&
  settlements.context===context&&settlements.target===accountTarget&&
  settlements.read_only===true&&settlements.orders_enabled===false&&
  settlements.next_stage_authorized===false&&settlements.breakeven_enabled===false;
 const settlementRows=settlementsBound&&Array.isArray(settlements.rows)?
  settlements.rows.slice(0,75).map(object):[];
 const commandLinks=object(view.partial_exit_command_links);
 const commandLinksBound=verified&&commandLinks.schema==='partial-exit-stage-command-links-v1'&&
  commandLinks.status==='broker_stage_links_read_only'&&commandLinks.context===context&&
  commandLinks.target===(t?.demo?'demo':'live')&&
  commandLinks.read_only===true&&commandLinks.orders_enabled===false&&
  commandLinks.stage_execution_supported===false;
 const linkedRows=commandLinksBound&&Array.isArray(commandLinks.rows)?
  commandLinks.rows.slice(0,75).map(object):[];
 const linkState:Record<string,string>={
  no_core_stage_command:'لا يوجد أمر إغلاق لهذه المرحلة صادر من Core بعد',
  conflicting_stage_commands:'أكثر من أمر مرتبط بنفس المرحلة — مرفوض',
  command_not_broker_reconciled:'أمر المرحلة لم تتم مصالحته مع الوسيط بعد',
  command_identity_mismatch:'بيانات أمر المرحلة لا تطابق الخطة والحساب',
  original_tp_price_lineage_not_verified:'سعر المرحلة أو الدخول أو TP الوسيط لا يطابق الخطة الأصلية المحفوظة',
  gateway_history_proof_missing:'تأكيد Gateway غير مدعوم بتاريخ التنفيذ النهائي',
  gateway_order_ticket_missing:'رقم أمر MT5 الفعلي مفقود',
  gateway_request_not_matching_stage:'الحجم أو الاتجاه المرسل لا يطابق المرحلة',
  broker_order_claimed_by_multiple_stages:'رقم أمر الوسيط مرتبط بأكثر من مرحلة',
  gateway_deal_ticket_list_missing:'تذاكر صفقات تنفيذ الأمر غير متوفرة',
  gateway_primary_deal_ticket_missing:'التذكرة الرئيسية لتنفيذ الأمر غير متاحة',
  gateway_primary_deal_not_in_fill_batch:'تذكرة التنفيذ الرئيسية غير موجودة ضمن صفقات الأمر',
  matching_broker_deal_not_yet_synced:'الصفقات المقابلة لم تُحفظ في Core بعد',
  broker_deal_order_or_identity_mismatch:'الصفقات لا تطابق رقم الأمر أو هوية المركز',
  broker_fill_lots_do_not_cover_stage:'كميات الإغلاق المسجلة لا تغطي حجم المرحلة',
  broker_deal_history_not_synced:'تاريخ الوسيط لم يُصالح بعد',
  broker_order_deals_exactly_linked:'رقم الأمر والصفقات متطابقة في التاريخ — تحقق تاريخي فقط، لا تنفيذ آلي للمرحلة',
 };
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
  <p>تقسيم اللوت إلى 50%، 30%، 20%. يمكن حفظ أسعار الأهداف على حساب Demo أو Live الذي اخترته ورصد عبور TP1 في السجل البحثي، لكن لا تُرسل أوامر إغلاق تلقائية ولا يعتبر الرصد تنفيذ صفقة. هذه ليست خاصية TP جزئي جاهزة للتداول.</p>
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
   {nextGateBound?<details className="operator-advanced" aria-label="شروط انتقال TP2 وTP3 التاريخية">
     <summary>فحص شروط TP2 / TP3 بعد إغلاق موثق — شادو فقط</summary>
     <p className="safe-note">لا يُفحص هدف لاحق إلا إذا وثّق Core إغلاق المرحلة السابقة بأمر MT5 والـDeals وحجم المركز المتبقي. حتى لو اكتملت الشروط لا يرسل النظام أمراً ولا ينقل SL إلى التعادل، وتبقى خطة الحجم الأصلية بحاجة مراجعة.</p>
     {nextGateRows.length===0?<p>لم تُثبت تسوية مرحلة سابقة من الوسيط بعد، لذلك لا يوجد هدف تالٍ مؤهل حتى للدراسة.</p>:
      nextGateRows.map((r,i)=>{
       const stage=[2,3].includes(Number(r.stage))?Number(r.stage):null;
       const ready=stage!==null&&r.state==='historical_prerequisites_met_shadow_only'&&
        r.historical_condition_only===true&&r.read_only===true&&
        r.orders_enabled===false&&r.next_stage_authorized===false&&
        r.breakeven_enabled===false&&valid(r.expected_close_lots);
       return <div className="brain-control-row" key={String(r.plan_id)+'-'+i}>
        <div><b>{stage?'TP'+stage:'TP غير معروف'} — مركز {Number.isSafeInteger(r.position_identifier)?String(r.position_identifier):'—'}</b>
         <small style={{display:'block',marginTop:4}}>{ready?
          'شروط التاريخ والسعر مكتملة للدراسة فقط؛ التنفيذ ما زال مقفلاً':
          r.state==='unverified_or_competing_close_command'?
          'هناك إغلاق سابق غير محسوم أو متعارض — ممنوع الاعتماد':
          r.state==='broker_quote_not_after_previous_settlement'?
          'انتظر اقتباس MT5 أصلياً جديداً بعد تسوية المرحلة السابقة':
          r.state==='next_target_not_crossed'?
          'لم يصل سعر الإغلاق إلى الهدف التالي':
          'شروط تاريخ الوسيط أو المركز غير مكتملة — يتطلب مراجعة'}</small>
        </div>
        <span>{ready?'لوت مفترض '+volume(r.expected_close_lots):'غير مؤهل'}</span>
       </div>;
      })}
    </details>:null}
   {intentBound?<details className="operator-advanced" aria-label="مسودات حجم الإغلاق التجريبي TP1">
     <summary>مسودة حجم إغلاق TP1 — معاينة فقط</summary>
     <p className="safe-note">يحسب Rust Core حجم الإغلاق المتوقع من خطة اللوت المحفوظة بعد عبور TP1 بالشادو، ويتحقق من المركز وخطوة الحجم وقواعد الوسيط. لا تُسجّل المسودة ضمن أوامر Core أو Gateway، ولا تفتح أو تغلق مركزاً ولا تحقق ربحاً.</p>
     {intentRows.length===0?<p>لم تُنشأ مسودة إغلاق بحثية بعد. يجب توفر خطة مسعّرة وعبور جديد موثق من MT5.</p>:
      intentRows.map((r,i)=>{
       const proven=r.state==='shadow_close_intent_only'&&r.read_only===true&&
        r.orders_enabled===false&&r.broker_fill_verified===false&&
        r.next_stage_authorized===false&&r.breakeven_enabled===false&&
        valid(r.close_lots)&&valid(r.expected_remaining_volume);
       return <div className="brain-control-row" key={String(r.plan_id)+'-'+i}>
        <div><b>TP1 — مركز {Number.isSafeInteger(r.position_identifier)?String(r.position_identifier):'—'}</b>
         <small style={{display:'block',marginTop:4}}>{proven?
          'مسودة لوت بحثية فقط — لم يُرسل أمر إلى MT5':
          'المركز أو سجل الخطة تغير أو سند المسودة غير صالح — يحتاج مراجعة'}</small>
        </div>
        <span>{proven?'إغلاق '+volume(r.close_lots)+' / متبقٍ '+volume(r.expected_remaining_volume)+' لوت':'غير مؤكد'}</span>
       </div>;
      })}
    </details>:null}
   {shadowBound?<details className="operator-advanced" aria-label="رصد TP1 البحثي من اقتباسات MT5">
     <summary>رصْد عبور TP1 من سعر MT5 — شادو فقط</summary>
     <p className="safe-note">يحفظ Core أول عبور موثق لـ Bid عند BUY أو Ask عند SELL بعد حفظ الأسعار، وفق وقت الاقتباس الأصلي من MT5. هذا ليس أمراً للوسيط، ولا يعني تنفيذ هدف أو نجاح صفقة أو السماح بـ TP2.</p>
     {shadowRows.length===0?<p>لا توجد عبورات TP1 مرصودة وموثقة بعد. قد يكون الهدف بعيداً أو لا تتوفر اقتباسات MT5 حديثة بمصدر موثوق.</p>:
      shadowRows.map((r,i)=>{
       const validShadow=r.state==='historical_shadow_crossing_only'&&
        r.orders_enabled===false&&r.broker_fill_verified===false&&
        r.next_stage_authorized===false&&r.breakeven_enabled===false&&
        Number.isSafeInteger(r.source_quote_time_ms)&&valid(r.observed_price)&&valid(r.target_price);
       return <div className="brain-control-row" key={String(r.plan_id)+'-'+i}>
        <div><b>مركز {Number.isSafeInteger(r.position_identifier)?String(r.position_identifier):'—'} — TP1</b>
         <small style={{display:'block',marginTop:4}}>{validShadow?
          'عبور سعري بحثي مؤرشف فقط، لا يوجد أي إغلاق مؤكد':
          'سند الرصد السابق يحتاج مراجعة؛ لا يُعتمد كعبور صالح'}</small>
        </div>
        <span>{validShadow?'السعر '+volume(r.observed_price)+' / الهدف '+volume(r.target_price):'غير موثق'}</span>
       </div>;
      })}
    </details>:null}
   {settlementsBound?<details className="operator-advanced" aria-label="تسوية مراحل TP الموثقة مع المركز">
     <summary>تسوية TP1/TP2/TP3 مع حجم المركز الحقيقي — قراءة فقط</summary>
     <p className="safe-note">التسوية تحتاج سجل أمر الوسيط وصفقات Deal مطابقة مع لقطة مركز MT5 الكاملة بعد كل مرحلة، بالتسلسل. توثيق مرحلة سابقة لا يسمح بتنفيذ التالية ولا ينقل الوقف إلى التعادل.</p>
     {settlementRows.length===0?<p>لا توجد خطط TP محفوظة للتسوية في هذا الحساب.</p>:
      settlementRows.map((r,i)=>{
       const sealed=r.state==='historical_stage_sealed'&&
        r.next_stage_authorized===false&&r.breakeven_triggered===false&&
        Number.isSafeInteger(r.broker_order)&&valid(r.matched_volume);
       return <div className="brain-control-row" key={String(r.plan_id)+'-'+i}>
        <div><b>TP{[1,2,3].includes(Number(r.stage))?String(r.stage):'؟'} — مركز {Number.isSafeInteger(r.position_identifier)?String(r.position_identifier):'—'}</b>
         <small style={{display:'block',marginTop:4}}>{sealed?
          'مطابقة تاريخية محفوظة: أمر MT5 والـDeals وحجم المركز المتبقي':
          r.state==='sealed_evidence_needs_review'?
          'التسوية القديمة محفوظة لكن سند الوسيط الحالي تغيّر أو فقد — يلزم تدقيق ولا يُسمح بالمرحلة التالية':
          'بانتظار إثبات سجل الوسيط ولقطة المركز المتبقية'}</small>
        </div>
        <span>{sealed?volume(r.matched_volume)+' لوت مُصالح':'غير مُصالح'}</span>
       </div>;
      })}
    </details>:null}
   {commandLinksBound?<details className="operator-advanced" aria-label="سلسلة دليل ربط مراحل TP بالأوامر">
     <summary>مطابقة مرحلة TP مع أمر Core وصفقات MT5 — للقراءة فقط</summary>
     <p className="safe-note">نجاح المطابقة التاريخية يحتاج هوية الخطة وأمر Core الأصلي ورقم أمر MT5 وتذاكر Deals الفعلية وحجمها. لا يُفعّل هذا العرض المرحلة التالية أو ينقل وقف الخسارة للتعادل.</p>
     {linkedRows.length===0?<p>لا توجد مراحل TP مؤهلة للتدقيق.</p>:
      linkedRows.map((r,i)=>{const e=object(r.evidence);
       const exact=e.state==='broker_order_deals_exactly_linked'&&e.stage_completed===false&&e.orders_enabled===false;
       return <div className="brain-control-row" key={String(r.plan_id)+'-'+i}>
        <div><b>TP{[1,2,3].includes(Number(r.stage))?String(r.stage):'؟'} — مركز {Number.isSafeInteger(r.position_identifier)?String(r.position_identifier):'—'}</b>
         <small style={{display:'block',marginTop:4}}>{linkState[String(e.state)]||'دليل التنفيذ غير مكتمل أو غير صالح'}</small>
        </div>
        <span>{exact&&valid(e.matched_broker_lots)?volume(e.matched_broker_lots)+' لوت موثق':'غير مؤكد'}</span>
       </div>;
      })}
    </details>:null}
   {dealAuditBound?<details className="operator-advanced" aria-label="دليل الإغلاقات الفعلية من MT5">
     <summary>مراجعة إغلاقات MT5 الموثقة — لا تثبت تحقق TP</summary>
     <p className="safe-note">المبالغ هنا من سجل Broker Deals المحفوظ في Core، وليست أرقام الشادو أو المحاكاة. إغلاق المركز بحد ذاته لا يثبت أنه نُفّذ ضمن TP1/2/3؛ لا توجد أي مرحلة مذكورة كمنفذة.</p>
     {dealRows.length===0?<p>لا توجد مراكز مخططة يمكن فحص سجل إغلاقاتها.</p>:
      dealRows.map((r,i)=>{const e=object(r.evidence);
       const usable=e.tp_stage_attribution_verified===false&&e.stage_completed===false;
       const booked=usable&&
        (e.state==='broker_partial_exit_unattributed'||e.state==='broker_flat_by_deals_unattributed')&&
        typeof e.booked_net_account_currency==='number'&&Number.isFinite(e.booked_net_account_currency);
       return <div className="brain-control-row" key={String(r.position_identifier)+'-'+i}>
        <div><b>مركز {Number.isSafeInteger(r.position_identifier)?String(r.position_identifier):'—'}</b>
         <small style={{display:'block',marginTop:4}}>
          {usable?(dealState[String(e.state)]||'نتيجة السجل غير موثقة'):'نتيجة غير آمنة أو مجهولة — لا يمكن تأكيد تنفيذ TP'}
         </small>
         {booked?<small style={{display:'block',marginTop:4}}>
          صافي قيود الوسيط المحفوظة (ربح/خسارة + عمولة + سواب + رسوم):
          {' '}{(e.booked_net_account_currency as number).toLocaleString('en-US',{maximumFractionDigits:2})}
          {' '}{t?.currency||''} — ليس ربح TP
         </small>:null}
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
