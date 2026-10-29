import {useEffect,useMemo,useState} from 'react';
import {BarChart3,CalendarDays,Download,FileText,Filter,Printer,RefreshCw,Target,TrendingUp} from 'lucide-react';
import type {Status} from './types';
import {hasTauri,readPanel,readTradeHistory,saveExcelReport} from './bridge';
import {journalEvidence} from './journal-evidence';
import {brokerReport} from './broker-report';
import {reportSeries} from './report-series';
import {journalCsv,journalXlsx} from './journal-export';

type Slot={scope:string;execution:unknown;journal:unknown};
const fmt=(n:number|null)=>n===null?'—':n.toLocaleString('en-US',{minimumFractionDigits:2,maximumFractionDigits:2});
type Filters={from:string;to:string;mode:string;brain:string};
const emptyFilters:Filters={from:'',to:'',mode:'all',brain:'all'};

export function BrokerReportOperations({status,scope,preview,onJournal}:{
 status:Status|null;scope:string;preview:boolean;onJournal:()=>void;
}){
 const [slot,setSlot]=useState<Slot|null>(null);
 const [revision,setRevision]=useState(0);
 const [error,setError]=useState('');
 const [exportBusy,setExportBusy]=useState(false),[exportMessage,setExportMessage]=useState('');
 const [draft,setDraft]=useState<Filters>(emptyFilters),[filters,setFilters]=useState<Filters>(emptyFilters);
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
 const allRows=evidence?.ready?evidence.rows:[];
 const filtered=allRows.filter(row=>{
  if(filters.mode!=='all'&&row.mode!==filters.mode)return false;
  if(filters.brain!=='all'&&row.brain!==filters.brain)return false;
  if(filters.from||filters.to){
   const time=Date.parse(row.latestDeal);if(!Number.isFinite(time))return false;
   const date=new Date(time).toISOString().slice(0,10);
   if(filters.from&&date<filters.from)return false;
   if(filters.to&&date>filters.to)return false;
  }
  return true;
 });
 const report=brokerReport(status,evidence?.ready?{...evidence,rows:filtered}:evidence);
 const series=useMemo(()=>report.ready?reportSeries(filtered):[],[report.ready,filtered]);
 const currency=status?.telemetry?.currency||'';
 const modes=[...new Set(allRows.map(r=>r.mode))].sort(),brains=[...new Set(allRows.map(r=>r.brain))].sort();
 const validRows=report.ready?filtered.filter(r=>r.status==='CLOSED'&&r.provenCosts&&r.net!==null):[];
 const profits=validRows.filter(r=>r.net!>0),losses=validRows.filter(r=>r.net!<0);
 const grossWin=profits.reduce((n,r)=>n+r.net!,0),grossLoss=losses.reduce((n,r)=>n+Math.abs(r.net!),0);
 const factor=grossLoss>0&&Number.isFinite(grossWin/grossLoss)?grossWin/grossLoss:null;
 const averageWin=profits.length&&Number.isFinite(grossWin/profits.length)?grossWin/profits.length:null;
 const averageLoss=losses.length&&Number.isFinite(grossLoss/losses.length)?grossLoss/losses.length:null;
 const minimum=series.length?Math.min(0,...series.map(x=>x.total)):0,maximum=series.length?Math.max(0,...series.map(x=>x.total)):0;
 const range=Math.max(maximum-minimum,0.01),xy=series.map((v,i)=>({x:20+(series.length===1?0:i/(series.length-1))*620,y:155-(v.total-minimum)/range*130}));
 const line=xy.map(p=>`${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' ');
 const daily=series.reduce<Record<string,number>>((a,p)=>{a[p.label]=(a[p.label]||0)+p.net;return a},{});
 const maxDaily=Math.max(1,...Object.values(daily).map(Math.abs));
 const grouped=(field:'mode'|'brain')=>{
  const map=new Map<string,number>();for(const r of validRows){const value=(map.get(r[field])||0)+r.net!;if(!Number.isFinite(value))return [];map.set(r[field],value)}
  return [...map.entries()].sort((a,b)=>b[1]-a[1]).slice(0,6);
 };
 const exportRows=()=>{
  if(!report.ready||!filtered.length)return;
  const url=URL.createObjectURL(new Blob([journalCsv(filtered)],{type:'text/csv;charset=utf-8'}));
  const a=document.createElement('a');a.href=url;a.download='braintrade-report-sample.csv';a.click();window.setTimeout(()=>URL.revokeObjectURL(url),1000);
 };
 const exportExcel=async()=>{
  if(!report.ready||!filtered.length||!status?.telemetry||!hasTauri()||exportBusy)return;
  setExportBusy(true);setExportMessage('');
  try{
   const t=status.telemetry;
   const bytes=journalXlsx(filtered,`MT5 ${t.login} · ${t.server} · ${t.symbol} · ${t.currency} · ${t.demo?'DEMO':'REAL'} | عينة آخر 100 مركز محفوظ بعد الفلترة فقط`);
   const result=await saveExcelReport(bytes,'report') as {saved?:boolean;cancelled?:boolean};
   setExportMessage(result.saved?'تم حفظ عينة التقرير في ملف Excel':result.cancelled?'أُلغي حفظ ملف Excel':'تعذر تأكيد حفظ ملف Excel');
  }catch(e){setExportMessage('تعذر تصدير ملف Excel: '+String(e))}
  finally{setExportBusy(false)}
 };
 return <section className="operator-card concept-report" aria-label="Verified MT5 broker performance report">
  <div className="concept-report-header"><div><h3><Target size={24}/> التحليلات والتقارير</h3><p>تحليلات من إيصالات MT5 المغلقة والمثبتة فقط</p></div>
   <div className="concept-report-filters"><label>من <input aria-label="من تاريخ UTC" type="date" value={draft.from} onChange={e=>setDraft({...draft,from:e.target.value})}/></label><label>إلى <input aria-label="إلى تاريخ UTC" type="date" value={draft.to} onChange={e=>setDraft({...draft,to:e.target.value})}/></label>
    <label>الوضع <select aria-label="تصفية وضع التداول" value={draft.mode} onChange={e=>setDraft({...draft,mode:e.target.value})}><option value="all">الكل</option>{modes.map(m=><option key={m} value={m}>{m}</option>)}</select></label>
    <label>العقل <select aria-label="تصفية العقل" value={draft.brain} onChange={e=>setDraft({...draft,brain:e.target.value})}><option value="all">الكل</option>{brains.map(b=><option key={b} value={b}>{b}</option>)}</select></label>
    <button type="button" onClick={()=>setFilters(draft)} disabled={preview||!evidence?.ready||Boolean(draft.from&&draft.to&&draft.from>draft.to)}><Filter size={15}/> تطبيق</button>
   </div>
  </div>
  <p className="safe-note">آخر 100 مركز محفوظ فقط من Core؛ الفلاتر والتقارير تخص هذه العينة وتستخدم آخر وقت Deal بتوقيت UTC. لا تشمل Paper أو Shadow. لا تظهر نسبة النجاح إلا بعد 30 مركزاً مغلقاً موثقاً.</p>
  <div className="concept-report-metrics">
   {[[<TrendingUp/>, 'صافي الأرباح',report.net===null?'—':fmt(report.net)+' '+currency], [<FileText/>, 'الصفقات المغلقة',report.ready?String(report.fullyClosed):'—'], [<Target/>, 'نسبة النجاح',report.winRate===null?'—':report.winRate.toFixed(1)+'%'], [<BarChart3/>, 'أقصى سحب للحساب','—'], [<BarChart3/>, 'معامل الربح للعينة',factor===null?'—':fmt(factor)], [<TrendingUp/>, 'متوسط الربح',averageWin===null?'—':fmt(averageWin)], [<TrendingUp/>, 'متوسط الخسارة',averageLoss===null?'—':fmt(averageLoss)]].map(([icon,label,value],i)=><article key={String(label)}><span>{icon}</span><div><small>{label}</small><strong>{value}</strong><em>{i===3?'سجل Equity غير متاح':'العينة الموثقة'}</em></div></article>)}
  </div>
  {preview?<p className="safe-note">DESIGN PREVIEW — لا توجد نتائج وسيط فعلية في وضع المعاينة.</p>:
   !slot?<p className="safe-note">{error?'تعذر تحميل التقرير: '+error:'جار تحميل إيصالات MT5 من Core...'}</p>:
   !report.ready?<p className="safe-note">{report.reason}</p>:
   <p className="safe-note">{report.mode} · {report.reason}. مقروء ضمن الفلاتر: {report.sampled}، مغلق موثق: {report.fullyClosed}، غير مكتمل: {report.unverified}. {report.complete?'العينة تغطي المراكز المسجلة الحالية في Core.':'العينة ليست إجمالي أرباح الحساب.'}</p>}
  <div className="concept-report-charts">
   <article className="concept-report-card concept-running"><h4><TrendingUp size={18}/> صافي عينة الصفقات المغلقة عبر الوقت <small>ليس منحنى Equity للحساب</small></h4>
    {series.length>1?<svg viewBox="0 0 660 180" role="img" aria-label="صافي نتائج الصفقات المغلقة في العينة"><line x1="20" x2="640" y1="25" y2="25"/><line x1="20" x2="640" y1="90" y2="90"/><line x1="20" x2="640" y1="155" y2="155"/><polygon points={`20,155 ${line} 640,155`}/><polyline points={line}/></svg>:<div className="concept-chart-empty">لا توجد نقاط زمنية موثقة كافية للرسم</div>}
    <div className="concept-chart-legend"><span>{series[0]?.label||'—'}</span><b>{series.length?fmt(series[series.length-1].total)+' '+currency:'—'}</b><span>{series.at(-1)?.label||'—'}</span></div>
   </article>
   <article className="concept-report-card"><h4><BarChart3 size={18}/> صافي النتائج اليومية <small>UTC · العينة الحالية</small></h4><div className="concept-daily-bars">{Object.entries(daily).slice(-16).map(([day,value])=><div key={day} title={`${day}: ${fmt(value)} ${currency}`}><i className={value>=0?'positive':'negative'} style={{height:`${Math.max(4,Math.abs(value)/maxDaily*90)}px`}}/><small>{day.slice(5)}</small></div>)}{!series.length&&<span>لا توجد إيصالات مكتملة</span>}</div></article>
   <article className="concept-report-card"><h4><Target size={18}/> نسبة نجاح العينة</h4><div className="concept-report-number">{report.winRate===null?'—':report.winRate.toFixed(1)+'%'}</div><p>رابحة {report.wins??'—'} · خاسرة {report.losses??'—'} · الحد الأدنى 30</p></article>
   <article className="concept-report-card"><h4><BarChart3 size={18}/> أقصى سحب للحساب</h4><div className="concept-chart-empty">غير متاح بدون سجل Equity موثق</div></article>
   {(['mode','brain'] as const).map(field=><article className="concept-report-card" key={field}><h4><BarChart3 size={18}/> {field==='mode'?'مقارنة الأوضاع':'مقارنة العقول'} <small>صافي العينة</small></h4><div className="concept-group-bars">{grouped(field).map(([name,value])=><div key={name}><span>{name}</span><i style={{width:`${Math.min(100,Math.abs(value)/Math.max(1,...grouped(field).map(x=>Math.abs(x[1])))*100)}%`}}/><b>{fmt(value)}</b></div>)}{!grouped(field).length&&<p>لا توجد نتائج موثقة.</p>}</div></article>)}
  </div>
  <div className="concept-report-actions"><button type="button" disabled={preview} onClick={()=>setRevision(x=>x+1)}><RefreshCw size={15}/> تحديث التقرير من Core</button><button type="button" onClick={onJournal}><FileText size={15}/> فتح سجل التداول الكامل</button><button type="button" disabled={preview||!report.ready||!filtered.length||!hasTauri()||exportBusy} onClick={()=>void exportExcel()}><Download size={15}/> تصدير Excel</button><button type="button" disabled={preview||!report.ready||!filtered.length} onClick={exportRows}><Download size={15}/> تصدير CSV</button><button type="button" onClick={()=>window.print()}><Printer size={15}/> طباعة / PDF</button><span><CalendarDays size={15}/> الحساب الحالي: {status?.telemetry?.symbol||'—'}</span></div>
  {exportMessage&&<p className="safe-note" role="status">{exportMessage}</p>}
 </section>;
}
