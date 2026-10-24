import {qualityObservation} from './execution-quality';
import type {QualityStatus} from './execution-quality';

const reasonLabels:Record<string,string>={
 insufficient_margin:'الهامش غير كافٍ',
 risk_policy:'حدود المخاطرة',
 invalid_volume:'اللوت/الحجم',
 invalid_stops:'وقف الخسارة/الستوب',
 trading_disabled:'التداول غير مسموح',
 price_or_quote:'السعر أو التيك',
 market_closed:'السوق مغلق',
 too_many_requests:'طلبات متكررة',
 filling_policy:'سياسة التعبئة',
 account_context:'هوية الحساب',
 authorization:'التخويل',
 broker_or_unclassified:'رفض وسيط غير مصنّف',
 cancelled_before_send_or_reconciliation:'إلغاء',
 unknown_execution:'غير محسوم',
 not_rejected:'ليس رفضاً'
};
const fmt=(v:number|null)=>v===null?'—':v.toLocaleString('en-US',{maximumFractionDigits:2});
export function ExecutionQualityOperations({status,report,disabled,onRefresh}:{
 status:QualityStatus|null;report:unknown;disabled:boolean;onRefresh:()=>void;
}){
 const observed=qualityObservation(status,report,Date.now());
 return <section className="operator-card" aria-label="Execution quality from Core">
  <h3>جودة التنفيذ وسبب رفض أوامر الوسيط</h3>
  <p>سجل الأوامر الحقيقي المرتبط بالحساب والرمز الحالي، وليس نتائج Paper أو Shadow. الحالات غير المحسومة ليست رفضاً ولا تُعاد إرسالها تلقائياً.</p>
  {observed.ready?<div>
   <div className="operator-grid">
    <div>أحدث الأوامر <strong>{fmt(observed.sampleSize)}</strong></div>
    <div>المرفوضة <strong>{fmt(observed.rejects)}</strong></div>
    <div>نتيجة مجهولة UNKNOWN <strong>{fmt(observed.unknown)}</strong></div>
    <div>تحتاج تسوية/تأكيد <strong>{fmt(observed.unresolved)}</strong></div>
    <div>Queue → Send P95 <strong>{fmt(observed.queueP95)} ms</strong></div>
    <div>MT5 Send P95 <strong>{fmt(observed.sendP95)} ms</strong></div>
    <div>دورة Gateway P95 <strong>{fmt(observed.totalP95)} ms</strong></div>
   </div>
   <h4>أسباب الأوامر المرفوضة</h4>
   {Object.keys(observed.categoryCounts).length?
    <div className="execution-binding-summary">{Object.entries(observed.categoryCounts).map(([key,count])=>
     <div key={key}><b>{reasonLabels[key]||'غير مصنّف'}</b> · {count}</div>)}</div>:
    <p className="safe-note">لا توجد حالات رفض ضمن نافذة الأوامر المقروءة.</p>}
   <div className="table-scroll">
    <table><thead><tr>{['العقل','النوع','الحالة','فئة النتيجة','Retcode','التاريخ'].map(k=><th key={k}>{k}</th>)}</tr></thead>
     <tbody>{observed.rows.length?observed.rows.map((r,i)=>
      <tr key={i}><td>{r.brain}</td><td>{r.kind}</td><td>{r.state}</td>
       <td>{reasonLabels[r.category]||'غير مصنّف'}</td><td>{r.retcode??'—'}</td>
       <td>{new Date(r.createdMs).toLocaleString('en-GB')}</td></tr>):
       <tr><td colSpan={6}>لا توجد أوامر وسيط محفوظة ضمن هذه العينة.</td></tr>}</tbody></table>
   </div>
  </div>:<p className="safe-note">{observed.reason}. لا يتم عرض إحصائيات من حساب آخر أو مصدر قديم.</p>}
  <p className="safe-note">P95 محسوبة من قيم زمن مسجّلة فعلياً فقط، ولا تشمل بالضرورة زمن الشبكة الكلي أو التأكيد النهائي للوسيط. نجاح الإرسال ليس إغلاقاً رابحاً؛ سجل الصفقات المغلقة في صفحة Journal منفصل.</p>
  <div className="quick-ops"><button type="button" disabled={disabled} onClick={onRefresh}>تحديث جودة التنفيذ من Core</button></div>
 </section>;
}
