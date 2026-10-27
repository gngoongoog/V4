import type {Status} from './types';
import {quoteClockEvidence} from './quote-clock';

export function QuoteClockEvidence({status,preview}:{status:Status|null;preview:boolean}){
 const e=quoteClockEvidence(status,preview);
 const unavailable=e.state==='preview'?'معاينة تصميم — لا قياس حي':
  e.state==='legacy'?'Gateway قديم: وقت الوسيط الأصلي غير موثّق':
  e.state==='invalid'?'بيانات توقيت غير صالحة': 'بانتظار اتصال MT5 حديث';
 return <section className="safe-note" aria-label="دليل توقيت القناص">
  <strong>وقت سعر الوسيط وترتيب التيكات</strong>
  {e.state==='observed'?<p>وقت MT5 الأصلي: {e.source} · معرّف ترتيب الحدث: {e.event} · زيادة الترتيب: {e.offset}</p>:<p>{unavailable}</p>}
  <p>زيادة معرّف الحدث تميّز تغيّرات السعر بنفس الملي ثانية؛ ليست تأخير تنفيذ. هذه آخر لقطة مصالحة، ولا تثبت وصول جميع التيكات أو سرعة تعبئة الصفقة.</p>
 </section>;
}
