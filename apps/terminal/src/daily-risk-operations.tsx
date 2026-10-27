import {dailyRiskView} from './daily-risk-evidence';
import type {Status} from './types';

const fmt=(v:number|null,currency='')=>v===null?'—':
 v.toLocaleString('en-US',{minimumFractionDigits:2,maximumFractionDigits:2})+(currency?' '+currency:'');
export function DailyRiskOperations({status,monitor,preview}:{
 status:Status|null;monitor:unknown;preview:boolean;
}){
 const r=preview?null:dailyRiskView(status,monitor,Date.now());
 const cc=r?.accountCurrency||'';
 return <section className="operator-card" aria-label="Verified account daily-loss breaker">
  <h3>قاطع خسارة اليوم — الحساب بالكامل</h3>
  <p>حالة قاطع Core الحسابي لجميع الرموز من نفس حساب MT5. هذه قراءة مراقبة فقط ولا تغيّر إعدادات المخاطر أو تغلق أي مركز.</p>
  {preview?<p className="safe-note">DESIGN PREVIEW — لا توجد بيانات قاطع خسارة فعلية.</p>:
  !r?.verified?<p className="safe-note" role="status">{r?.message||'بانتظار Core'}</p>:
  <>
   <div className="execution-binding-summary" role="status">
    <strong>{r.mode} · {r.day} · {r.message}</strong>
    <small>القاطع: {r.enabled?'مفعّل':'مطفأ'} | القفل الدائم لهذا اليوم: {r.persistedLock?'نعم':'لا'}
     {' '}| حد الخسارة المختار: {fmt(r.limitPercent)}%</small>
   </div>
   <div className="operator-grid">
    <div>رصيد بداية اليوم المقدر<strong>{fmt(r.baseline,cc)}</strong></div>
    <div>الخسارة اليومية المقدرة<strong>{fmt(r.dayLoss,cc)}</strong></div>
    <div>حد خسارة اليوم المحسوب<strong>{fmt(r.allowedLoss,cc)}</strong></div>
    <div>المسافة التقديرية المتبقية للحد عند التفعيل<strong>{fmt(r.remaining,cc)}</strong></div>
    <div>نسبة الخسارة التقديرية<strong>{fmt(r.lossPercent)}%</strong></div>
   </div>
   <p className="safe-note">{r.warning}</p>
  </>}
 </section>;
}
