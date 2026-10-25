import {useEffect,useState} from 'react';
import type {Status} from './types';
import {readPanel,readTradeHistory} from './bridge';
import {journalEvidence} from './journal-evidence';
import {brokerReport} from './broker-report';

type Slot={scope:string;execution:unknown;journal:unknown};
const money=(n:number|null,currency:string)=>n===null?'—':
 n.toLocaleString('en-US',{minimumFractionDigits:2,maximumFractionDigits:2})+' '+currency;
export function BrokerReportOperations({status,scope,preview,onJournal}:{
 status:Status|null;scope:string;preview:boolean;onJournal:()=>void;
}){
 const [slot,setSlot]=useState<Slot|null>(null);
 const [revision,setRevision]=useState(0);
 const [error,setError]=useState('');
 useEffect(()=>{
  if(preview){setSlot(null);return}
  let alive=true;
  const run=async()=>{
   try{
    const [execution,journal]=await Promise.all([
     readPanel('/v1/execution'),readTradeHistory(0,100)]);
    if(alive){setSlot({scope,execution,journal});setError('')}
   }catch(e){if(alive){setSlot(null);setError(String(e))}}
  };
  setSlot(null);setError('');void run();
  return()=>{alive=false};
 },[scope,revision,preview]);
 const evidence=slot?.scope===scope?journalEvidence(status,slot.execution,slot.journal,0,100):null;
 const report=brokerReport(status,evidence);
 const currency=status?.telemetry?.currency||'';
 return <section className="operator-card" aria-label="Verified MT5 broker performance report">
  <h3>التقرير المالي من إيصالات MT5</h3>
  <p>يجمع آخر 100 مركز محفوظ فقط، ويحسب المراكز المغلقة بالكامل مع رسوم الوسيط المثبتة. لا يشمل Paper أو Shadow ولا الأرباح العائمة.</p>
  <div className="quick-ops">
   <button type="button" disabled={preview} onClick={()=>setRevision(x=>x+1)}>تحديث التقرير من Core</button>
   <button type="button" onClick={onJournal}>فتح سجل التداول الكامل</button>
  </div>
  {preview?<p className="safe-note">DESIGN PREVIEW — لا توجد نتائج وسيط فعلية في وضع المعاينة.</p>:
   !slot?<p className="safe-note">{error?'تعذر تحميل التقرير: '+error:'جار تحميل إيصالات MT5 من Core...'}</p>:
   !report.ready?<p className="safe-note">{report.reason}</p>:
   <>
    <p className="safe-note">{report.mode} · {report.reason}. عدد المراكز المقروءة: {report.sampled}،
     مغلقة موثّقة: {report.fullyClosed}، غير مكتملة أو مفتوحة: {report.unverified}.</p>
    <div className="operator-grid">
     <div>صافي الربح المحقق بعد التكاليف <strong>{money(report.net,currency)}</strong></div>
     <div>الربح قبل التكاليف <strong>{money(report.gross,currency)}</strong></div>
     <div>العمولات المحققة <strong>{money(report.commission,currency)}</strong></div>
     <div>Swap محقق <strong>{money(report.swap,currency)}</strong></div>
     <div>رسوم Fee <strong>{money(report.fee,currency)}</strong></div>
     <div>المراكز الرابحة / الخاسرة <strong>{report.wins===null?'—':report.wins+' / '+report.losses}</strong></div>
     <div>نسبة النجاح بعد الرسوم <strong>{report.winRate===null?'—':report.winRate.toFixed(1)+'%'}</strong></div>
    </div>
    <p className="safe-note">{report.complete?
     'العينة تغطي جميع المراكز المسجلة حالياً ضمن Core، وليست إثباتاً لاكتمال سجل الوسيط خارج نافذة المزامنة.':
     'النتيجة تخص أحدث 100 مركز فقط، وليست إجمالي أرباح الحساب. للحصول على بقية التاريخ افتح سجل التداول.'}
     {' '}لا تظهر نسبة النجاح إلا بعد 30 مركزاً مغلقاً موثقاً داخل العينة.
    </p>
   </>}
 </section>;
}
