import {useEffect,useState} from 'react';
import type {Status} from './types';
import {readPanel,readTradeHistory} from './bridge';
import {journalEvidence} from './journal-evidence';

type Read={scope:string;offset:number;limit:number;execution:unknown;history:unknown};
const fmt=(n:number|null)=>n===null?'—':n.toLocaleString('en-US',{maximumFractionDigits:2});
const stateLabel=(state:string)=>state==='CLOSED'?'إغلاق مثبت من أحجام Deals':
 state==='OPEN'?'مفتوحة':state==='PARTIAL / OPEN'?'إغلاق جزئي':
 state==='OPEN / AWAITING DEAL SYNC'?'تنتظر تحديث سجل MT5':'الخروج غير مثبت بالكامل';

export function TradeJournalOperations({status,scope,preview}:{
 status:Status|null;scope:string;preview:boolean;
}){
 const [offset,setOffset]=useState(0),[limit,setLimit]=useState(25);
 const [revision,setRevision]=useState(0);
 const [data,setData]=useState<Read|null>(null),[error,setError]=useState('');
 const [selected,setSelected]=useState<number|null>(null);
 useEffect(()=>{
  if(preview){setData(null);return;}
  let active=true;
  const fetch=async()=>{
   try{
    const [execution,history]=await Promise.all([
      readPanel('/v1/execution'),readTradeHistory(offset,limit)]);
    if(active){setData({scope,offset,limit,execution,history});setError('')}
   }catch(e){if(active){setData(null);setError(String(e))}}
  };
  setData(null);setSelected(null);setError('');
  void fetch();
  return()=>{active=false};
 },[scope,preview,offset,limit,revision]);
 const report=data?.scope===scope&&data.offset===offset&&data.limit===limit?
   journalEvidence(status,data.execution,data.history,offset,limit):null;
 const go=(next:number)=>{
  if(next<0||next>1000000||!Number.isSafeInteger(next))return;
  setOffset(next);
 };
 return <section className="operator-card" aria-label="MT5 broker trade journal">
  <h3>سجل صفقات MT5 الموثّق</h3>
  <p>سجل Core الفعلي؛ كل صف يمثل مركزاً برقم Position ID. الربح الصافي لا يظهر إلا بعد تحقق رسوم الوسيط، والصفقات الورقية غير مضمنة.</p>
  <div className="quick-ops">
   <label>عدد السجلات
    <select aria-label="عدد سجلات الصفحة" disabled={preview} value={limit}
     onChange={e=>{setLimit(Number(e.target.value));setOffset(0)}}>
     <option value={25}>25</option><option value={50}>50</option><option value={100}>100</option>
    </select>
   </label>
   <button type="button" disabled={preview} onClick={()=>setRevision(x=>x+1)}>تحديث من Core</button>
   <button type="button" disabled={preview||!report?.hasPrev}
    onClick={()=>go(Math.max(0,offset-limit))}>السابق</button>
   <button type="button" disabled={preview||!report?.hasNext}
    onClick={()=>go(offset+limit)}>التالي</button>
  </div>
  <p className="safe-note">
   {preview?'DESIGN PREVIEW — NO BROKER HISTORY':
    error?'خطأ قراءة السجل: '+error:
    !report?'جار تحميل صفحة السجل من Core...':
    !report.ready?report.reason:
    'صفحة '+String(Math.floor(offset/limit)+1)+' • محفوظ في Core: '+String(report.total)+' مركز • بداية السجل '+String(offset+1)}
  </p>
  {!preview&&report?.ready&&<div className="table-scroll">
   <table aria-label="مراكز التداول الموثقة">
    <thead><tr>{['Position ID','الرمز','الاتجاه','الحجم','حالة التسوية','الصافي بعد الرسوم','تفاصيل'].map(x=><th key={x}>{x}</th>)}</tr></thead>
    <tbody>{report.rows.length?report.rows.map(row=>
      <tr key={row.positionId}>
       <td>{row.positionId}</td><td>{row.symbol}</td><td>{row.side==='BUY'?'شراء':'بيع'}</td>
       <td>{fmt(row.lotsOpen)}</td><td>{stateLabel(row.status)}</td>
       <td>{row.net===null?'غير مثبت':fmt(row.net)+' '+(status?.telemetry?.currency||'')}</td>
       <td><button type="button" aria-expanded={selected===row.positionId}
        onClick={()=>setSelected(selected===row.positionId?null:row.positionId)}>تفاصيل</button>
        {selected===row.positionId&&<div className="broker-journal-detail">
         <div>وقت الدخول UTC: {row.opened}</div>
         <div>العقل: {row.brain}</div>
         <div>سبب الدخول: {row.reason}</div>
         <div>Opened: {fmt(row.lotsOpen)} | Closed: {fmt(row.lotsClosed)} | Remaining: {fmt(row.remaining)}</div>
         <div>Profit: {fmt(row.profit)} | Commission: {fmt(row.commission)} | Swap: {fmt(row.swap)} | Fee: {fmt(row.fee)}</div>
         <strong>{row.provenCosts?'الرسوم مسجلة من Broker Deals':'غير مكتمل: بعض مكونات الرسوم لم تؤكدها إيصالات MT5'}</strong>
        </div>}
       </td>
      </tr>):<tr><td colSpan={7}>لا توجد مراكز موثقة في هذه الصفحة.</td></tr>}</tbody>
   </table>
  </div>}
  <p className="safe-note">يُحسب إجمالي المراكز المسجلة من قاعدة Core، وقد تضاف مراكز حية لم تُزامن Deals الخاصة بها بعد. لا يمكن لهذه الصفحة إرسال أو إغلاق صفقات.</p>
 </section>;
}
