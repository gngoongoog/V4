import {useEffect,useState} from 'react';
import {BarChart3,Clock,Download,FileText,RefreshCw,Search,Target} from 'lucide-react';
import type {Status} from './types';
import {readPanel,readTradeHistory} from './bridge';
import {journalEvidence} from './journal-evidence';
import {brokerReport} from './broker-report';
import {journalCsv} from './journal-export';

type Read={scope:string;offset:number;limit:number;execution:unknown;history:unknown};
const fmt=(n:number|null)=>n===null?'—':n.toLocaleString('en-US',{maximumFractionDigits:2});
const stateLabel=(state:string)=>state==='CLOSED'?'مغلقة':state==='OPEN'?'مفتوحة':
 state==='PARTIAL / OPEN'?'إغلاق جزئي':state==='OPEN / AWAITING DEAL SYNC'?'تنتظر سجل MT5':'غير محسومة';

export function TradeJournalOperations({status,scope,preview}:{
 status:Status|null;scope:string;preview:boolean;
}){
 const [offset,setOffset]=useState(0),[limit,setLimit]=useState(12);
 const [revision,setRevision]=useState(0);
 const [data,setData]=useState<Read|null>(null),[error,setError]=useState('');
 const [selected,setSelected]=useState<number|null>(null);
 const [query,setQuery]=useState(''),[side,setSide]=useState<'all'|'BUY'|'SELL'>('all');
 const [outcome,setOutcome]=useState<'all'|'win'|'loss'|'open'>('all');
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
 const summary=brokerReport(status,report);
 const rows=report?.ready?report.rows:[];
 const filtered=rows.filter(row=>{
  const text=[row.positionId,row.symbol,row.side,row.brain,row.mode,row.reason].join(' ').toLowerCase();
  if(query.trim()&&!text.includes(query.trim().toLowerCase()))return false;
  if(side!=='all'&&row.side!==side)return false;
  if(outcome==='win'&&!(row.status==='CLOSED'&&row.net!==null&&row.net>0))return false;
  if(outcome==='loss'&&!(row.status==='CLOSED'&&row.net!==null&&row.net<0))return false;
  if(outcome==='open'&&row.status==='CLOSED')return false;
  return true;
 });
 const detail=rows.find(row=>row.positionId===selected)??null;
 const go=(next:number)=>{
  if(next<0||next>1000000||!Number.isSafeInteger(next))return;
  setOffset(next);
 };
 const exportPage=()=>{
  if(!report?.ready||!filtered.length)return;
  const blob=new Blob([journalCsv(filtered)],{type:'text/csv;charset=utf-8'});
  const url=URL.createObjectURL(blob),link=document.createElement('a');
  link.href=url;link.download='braintrade-mt5-journal-page.csv';link.click();
  window.setTimeout(()=>URL.revokeObjectURL(url),1000);
 };
 const currency=status?.telemetry?.currency||'';
 const message=preview?'معاينة التصميم: لا يُعرض سجل وسيط فعلي':error?'خطأ قراءة السجل: '+error:
    !report?'جار تحميل صفحة السجل من Core...':!report.ready?report.reason:
    'السجل مرتبط بالحساب الحالي. الفلاتر والتصدير يطبقان على الصفحة المعروضة فقط.';
 return <section className="operator-card concept-journal" aria-label="MT5 broker trade journal">
  <div className="concept-journal-head"><div><h3><FileText size={23}/> سجل العمليات</h3><p>عرض الصفقات المسجلة وإيصالات التنفيذ الفعلية</p></div><div><button type="button" disabled={preview||!report?.ready||!filtered.length} onClick={exportPage}><Download size={15}/> تصدير CSV لـExcel</button><button type="button" disabled={preview} onClick={()=>setRevision(x=>x+1)}><RefreshCw size={15}/> تحديث</button></div></div>
  <div className="concept-journal-metrics">
   <div><FileText/><span>المراكز المسجلة<strong>{report?.ready?report.total:'—'}</strong><small>في سجل Core الحالي</small></span></div>
   <div><BarChart3/><span>صافي عينة الصفحة<strong>{summary.ready?fmt(summary.net):'—'} {summary.net!==null?currency:''}</strong><small>بعد الرسوم الموثقة</small></span></div>
   <div><Target/><span>نسبة نجاح العينة<strong>{summary.winRate===null?'—':summary.winRate.toFixed(1)+'%'}</strong><small>تظهر بعد 30 مركزاً مؤهلاً</small></span></div>
   <div><Download/><span>عمولات العينة<strong>{summary.ready?fmt(summary.commission):'—'} {summary.commission!==null?currency:''}</strong><small>من إيصالات الوسيط</small></span></div>
   <div><Clock/><span>نتائج موثقة<strong>{summary.ready?summary.fullyClosed:'—'}</strong><small>في الصفحة الحالية فقط</small></span></div>
  </div>
  <div className="concept-journal-filters"><label><Search size={16}/><input aria-label="البحث في صفحة السجل" placeholder="البحث بالرمز أو رقم المركز أو العقل..." value={query} onChange={e=>setQuery(e.target.value)}/></label>
   <select aria-label="اتجاه الصفقة" value={side} onChange={e=>setSide(e.target.value as typeof side)}><option value="all">كل الاتجاهات</option><option value="BUY">شراء</option><option value="SELL">بيع</option></select>
   <div className="concept-outcome-filters">{([['all','جميع النتائج'],['win','الرابحة'],['loss','الخاسرة'],['open','غير مغلقة']] as const).map(([id,label])=><button type="button" key={id} aria-pressed={outcome===id} className={outcome===id?'active':''} onClick={()=>setOutcome(id)}>{label}</button>)}</div>
  </div>
  <p className="safe-note" role="status">{message}</p>
  <div className="concept-journal-body"><div className="table-scroll">
   <table aria-label="مراكز التداول الموثقة"><thead><tr>{['#','الوقت UTC','الرمز','الوضع','العقل','الاتجاه','الحجم','سعر الدخول','العمولة','النتيجة','الحالة'].map(x=><th key={x}>{x}</th>)}</tr></thead>
    <tbody>{filtered.length?filtered.map(row=><tr key={row.positionId} className={selected===row.positionId?'selected':''} onClick={()=>setSelected(row.positionId)}>
     <td><button type="button" aria-expanded={selected===row.positionId} onClick={e=>{e.stopPropagation();setSelected(selected===row.positionId?null:row.positionId)}}>{row.positionId}</button></td>
     <td>{row.opened}</td><td>{row.symbol}</td><td>{row.mode}</td><td>{row.brain}</td><td className={row.side==='BUY'?'teal':'red'}>{row.side}</td>
     <td>{fmt(row.lotsOpen)}</td><td>{fmt(row.entryPrice)}</td><td>{fmt(row.commission)}</td><td className={row.net===null?'':row.net>=0?'teal':'red'}>{row.net===null?'غير مثبت':fmt(row.net)}</td><td>{stateLabel(row.status)}</td>
    </tr>):<tr><td colSpan={11}>{report?.ready?'لا توجد سجلات تطابق فلتر الصفحة.':'لا توجد بيانات وسيط مؤكدة للعرض.'}</td></tr>}</tbody>
   </table>
  </div><aside className="concept-journal-detail"><h3>تفاصيل العملية</h3>{detail?<>
   <strong>#{detail.positionId} · {stateLabel(detail.status)}</strong><h4>{detail.symbol} <span className={detail.side==='BUY'?'teal':'red'}>{detail.side}</span></h4>
   <dl><div><dt>وقت الدخول UTC</dt><dd>{detail.opened}</dd></div><div><dt>سعر الدخول</dt><dd>{fmt(detail.entryPrice)}</dd></div><div><dt>الحجم</dt><dd>{fmt(detail.lotsOpen)}</dd></div><div><dt>الحجم المغلق</dt><dd>{fmt(detail.lotsClosed)}</dd></div><div><dt>العمولة</dt><dd>{fmt(detail.commission)}</dd></div><div><dt>الصافي بعد الرسوم</dt><dd>{detail.net===null?'غير مثبت':fmt(detail.net)+' '+currency}</dd></div><div><dt>العقل</dt><dd>{detail.brain}</dd></div><div><dt>الوضع</dt><dd>{detail.mode}</dd></div></dl><p>{detail.reason}</p><small>{detail.provenCosts?'الرسوم من إيصالات الوسيط':'لم تثبت كل مكونات الرسوم'}</small>
  </>:<p>اختر صفاً لعرض بياناته الموثقة.</p>}</aside></div>
  <div className="concept-journal-footer"><div><button type="button" disabled={preview||!report?.hasPrev} onClick={()=>go(Math.max(0,offset-limit))}>السابق</button><b>{Math.floor(offset/limit)+1}</b><button type="button" disabled={preview||!report?.hasNext} onClick={()=>go(offset+limit)}>التالي</button></div><label>عدد السجلات في الصفحة <select aria-label="عدد سجلات الصفحة" disabled={preview} value={limit} onChange={e=>{setLimit(Number(e.target.value));setOffset(0)}}><option value={12}>12</option><option value={25}>25</option><option value={50}>50</option><option value={100}>100</option></select></label></div>
  <p className="safe-note">الصافي يظهر فقط بعد إثبات مكونات رسوم الوسيط. هذه الصفحة للقراءة ولا ترسل أو تغلق صفقات.</p>
 </section>;
}
