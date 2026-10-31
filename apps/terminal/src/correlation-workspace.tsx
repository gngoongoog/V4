import './operator-pages.css';
import {useCallback,useEffect,useState} from 'react';
import {BarChart3,BookOpen,RefreshCw,ShieldAlert} from 'lucide-react';
import {correlationAction} from './bridge';

type Row=Record<string,unknown>;
const obj=(v:unknown):Row=>v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Row:{};
const arr=(v:unknown):Row[]=>Array.isArray(v)?v.filter(x=>x!==null&&typeof x==='object'&&!Array.isArray(x)):[];
const txt=(v:unknown,alt='—')=>typeof v==='string'&&v.trim()?v:alt;
const value=(v:unknown)=>typeof v==='number'&&Number.isFinite(v)?v:null;
const stamp=(v:unknown)=>value(v)!==null&&Number(v)>0?new Date(Number(v)).toLocaleString('ar-IQ'):'—';
/** Mirrors the domain's mandatory account/symbol constraints; Core remains final authority. */
export function sourceCompatibility(selected:Row[],candidate:Row):string|null{
 const sym=txt(candidate.symbol,''),server=txt(candidate.server,''),ccy=txt(candidate.currency,''),login=value(candidate.login);
 const from=value(candidate.from),to=value(candidate.to);
 if(!sym||!server||!ccy||login===null||from===null||to===null||from>=to)return 'المصدر ناقص: الرمز أو الحساب أو نطاق التاريخ غير صالح';
 if(selected.some(x=>x.symbol===sym))return 'الارتباط يتطلب رمزين مختلفين؛ تكرار '+sym+' غير مقبول';
 if(selected.some(x=>x.server!==server||x.login!==login||x.currency!==ccy))return 'يلزم نفس حساب MT5 والخادم وعملة الحساب لكل المصادر';
 const all=[...selected,candidate];
 if(all.length>=2){
  const earliestEnd=Math.min(...all.map(x=>Number(x.to)));
  const latestStart=Math.max(...all.map(x=>Number(x.from)));
  if(!(earliestEnd>latestStart))return 'الفترات الزمنية لا تتداخل؛ اختر نتائج من نفس الفترة';
 }
 return null;
}
export function CorrelationWorkspace({source,preview,disabled,error,onAnalyze,onRefresh,onBacktest}:{source:unknown;preview:boolean;disabled:boolean;error:string;onAnalyze:(ids:string[],overlap:number)=>Promise<unknown>;onRefresh:()=>void;onBacktest:()=>void}){
 const [sources,setSources]=useState<Row[]>([]),[reports,setReports]=useState<Row[]>([]),[chosen,setChosen]=useState<string[]>([]);
 const [overlap,setOverlap]=useState('30'),[active,setActive]=useState(''),[report,setReport]=useState<Row|null>(null);
 const [loading,setLoading]=useState(false),[busy,setBusy]=useState(false),[message,setMessage]=useState(''),[failure,setFailure]=useState('');
 const refresh=useCallback(async()=>{
  if(preview)return;
  setLoading(true);
  try{
   const [a,b]=await Promise.all([correlationAction('sources'),correlationAction('reports')]);
   const valid=arr(obj(a).sources),saved=arr(obj(b).reports);
   setSources(valid);setReports(saved);setChosen(prev=>{
    const retained:Row[]=[];const selectedIds:string[]=[];
    for(const id of prev){const item=valid.find(x=>x.id===id);if(item&&selectedIds.length<8&&!sourceCompatibility(retained,item)){retained.push(item);selectedIds.push(id)}}
    return selectedIds;
   });
   setFailure('');
  }catch(e){setFailure('تعذر جلب المصادر أو التقارير التاريخية: '+String(e))}
  finally{setLoading(false)}
 },[preview]);
 useEffect(()=>{void refresh()},[refresh]);
 const toggle=(id:string)=>{
  setChosen(current=>{
   if(current.includes(id))return current.filter(x=>x!==id);
   const candidate=sources.find(x=>x.id===id);
   if(!candidate||current.length>=8)return current;
   const currentRows=sources.filter(x=>current.includes(String(x.id)));
   const issue=sourceCompatibility(currentRows,candidate);
   if(issue){setFailure(issue);return current}
   setFailure('');return [...current,id];
  });
 };
 const analyze=async()=>{
  const n=Number(overlap);
  if(chosen.length<2||chosen.length>8){setFailure('اختر من رمزين مختلفين إلى ثمانية رموز محفوظة لنفس حساب MT5');return}
  const selectedRows=sources.filter(x=>chosen.includes(String(x.id)));
  const issue=selectedRows.reduce<string|null>((reason,item,index)=>reason??sourceCompatibility(selectedRows.slice(0,index),item),null);
  if(issue){setFailure(issue);return}
  if(!Number.isInteger(n)||n<2||n>60000){setFailure('حد التداخل: عدد صحيح بين 2 و60000 دقيقة');return}
  const start=Math.max(...selectedRows.map(x=>Number(x.from))),end=Math.min(...selectedRows.map(x=>Number(x.to)));
  if((end-start)/60<n){setFailure('الفترة المشتركة أصغر من حد التداخل المطلوب؛ اختر مصادر بفترة مشتركة أطول');return}
  setBusy(true);setFailure('');setMessage('');
  try{
   const res=obj(await onAnalyze(chosen,n));
   const summary=obj(res.summary);
   if(res.ok!==true||typeof summary.id!=='string')throw new Error('لا يوجد تأكيد حفظ من Core');
   setMessage('أكد Core حفظ دراسة الارتباط برقم '+summary.id+'. النتائج بحثية وليست تداولاً');
   onRefresh();await refresh();
  }catch(e){setFailure('لم يتأكد حفظ الدراسة: '+String(e))}
  finally{setBusy(false)}
 };
 const openReport=async(id:string)=>{
  setBusy(true);setFailure('');setReport(null);
  try{
   const r=obj(await correlationAction('report',undefined,undefined,id));
   if(r.id!==id||r.research_only!==true)throw new Error('تقرير الارتباط لا يطابق المصدر المختار');
   setActive(id);setReport(obj(r.report));
  }catch(e){setFailure('تعذر فتح التقرير المحفوظ: '+String(e))}
  finally{setBusy(false)}
 };
 const pairs=arr(report?.pairs);
 const summary=obj(source);
 const ready=!preview&&!disabled&&!loading&&!busy;
 const chosenSource=sources.filter(s=>chosen.includes(String(s.id)));
 const symbols=[...new Set(chosenSource.map(s=>txt(s.symbol)).filter(s=>s!=='—'))];
 return <section className="operator-workspace" aria-label="مختبر الارتباطات التاريخية" dir="rtl">
  <div className="operator-workspace-head"><div><h3><BarChart3 size={19}/> مختبر الارتباطات التاريخية</h3><p>مقارنة عوائد شموع M1 المتزامنة من اختبارات محفوظة، بدون تعويض الفجوات أو استخدام أسعار السوق الحية.</p></div><button type="button" onClick={()=>{onRefresh();void refresh()}} disabled={preview||loading||busy}><RefreshCw size={16}/> تحديث المصادر</button></div>
  <div className="operator-evidence">{preview?'معاينة: لا بيانات':error?'خطأ في قراءة Core: '+error:source===null?'بانتظار بيانات Core':summary.research_only===true?'Core يؤكد أن تحليل الارتباط بحثي فقط':'حدود مصدر الدراسة غير مؤكدة'}</div>
  {!preview&&<div className="operator-measures">
   <div className="operator-measure"><small>الاختبارات المحفوظة</small><b>{typeof summary.saved_backtests==='number'?summary.saved_backtests:'—'}</b><span>بقاعدة بيانات Core</span></div>
   <div className="operator-measure"><small>التقارير المحفوظة</small><b>{typeof summary.saved_reports==='number'?summary.saved_reports:'—'}</b><span>مصادر البحث</span></div>
   <div className="operator-measure"><small>مباشر مع السوق؟</small><b>{summary.live_feed_used===false?'لا': 'غير مؤكد'}</b><span>ليس عمق سوق حي</span></div>
   <div className="operator-measure"><small>أثر على التنفيذ؟</small><b>{summary.execution_effect===false?'لا':'غير مؤكد'}</b><span>لا يرسل أوامر MT5</span></div>
  </div>}
  <div className="operator-control-panel">
   <h4>١. حدد المصادر التاريخية</h4><p>اختر من 2 إلى 8 <strong>رموز مختلفة</strong> من <strong>نفس حساب MT5 والخادم وعملة الحساب</strong>، وبتواريخ متداخلة. الاختبارات القديمة للذهب XAUUSD وحده لا تكفي لإنشاء دراسة ارتباط بين رمزين.</p>
   {sources.length?<div className="operator-choose-grid">{sources.map((s,i)=>{
    const id=txt(s.id,'');
    const issue=chosen.includes(id)?null:sourceCompatibility(chosenSource,s);
    return <label key={id||String(i)} title={issue||''}><input type="checkbox" disabled={!ready||(!chosen.includes(id)&&(chosen.length>=8||Boolean(issue)))} checked={chosen.includes(id)} onChange={()=>toggle(id)}/><span><b>{txt(s.symbol,'رمز غير متاح')}</b><small>{txt(s.server,'الخادم غير معلوم')} · حساب {value(s.login)??'—'} · {txt(s.currency,'عملة مجهولة')}</small><small>الفترة: {value(s.from)!==null?new Date(Number(s.from)*1000).toISOString().slice(0,10):'—'} → {value(s.to)!==null?new Date(Number(s.to)*1000).toISOString().slice(0,10):'—'}</small>{issue&&<small>غير قابل للاختيار: {issue}</small>}</span></label>;
   })}</div>:<p className="operator-empty">{loading?'جاري تحميل المصادر...':'لا توجد اختبارات تاريخية محفوظة مؤهلة في Core.'}</p>}
   <div className="operator-filters"><label>التداخل الأدنى بين الشموع بالدقائق<input type="number" aria-label="حد التداخل بالارتباط" min="2" max="60000" step="1" value={overlap} onChange={e=>setOverlap(e.target.value)} disabled={!ready}/></label></div>
   {sources.length>0&&new Set(sources.map(s=>txt(s.symbol))).size<2&&<p className="safe-note">المحفوظ حالياً يحتوي رمزاً واحداً فقط؛ يلزم تاريخ M1 محفوظ لرمز ثانٍ من نفس الحساب لإجراء دراسة ارتباط. يمكن الاستمرار في تداول الذهب وحده؛ هذه ميزة بحثية اختيارية.</p>}
   {chosen.length===1&&<p className="safe-note">اختر رمزاً مختلفاً عن {symbols[0]||'المصدر الأول'} بنفس الحساب وفترة تتداخل معه.</p>}
   <div className="operator-links"><button type="button" disabled={!ready||chosen.length<2||sourceCompatibility(chosenSource.slice(0,1),chosenSource[1]||{})!==null} onClick={()=>void analyze()}>تحليل ارتباط المصادر المختارة</button><button type="button" onClick={onBacktest}><BookOpen size={15}/> إنشاء اختبار تاريخي</button></div>
  </div>
  <div className="operator-control-panel">
   <h4>٢. التقارير المحفوظة</h4>
   {reports.length?<div className="operator-choose-grid">{reports.map((item,i)=>{const s=obj(item.summary),id=txt(s.id,'');return <button key={id||i} className={active===id?'selected':''} disabled={!ready||!id} onClick={()=>void openReport(id)}><b>{(Array.isArray(s.symbols)?s.symbols.filter(x=>typeof x==='string').join(' / '):'')||'ارتباط محفوظ'}</b><small>المعرف {id.slice(0,12)} · {stamp(item.created_ms)} · أزواج: {String(s.pair_count??'—')}</small></button>})}</div>:<p className="operator-empty">لا توجد دراسات ارتباط محفوظة لعرضها.</p>}
  </div>
  {failure&&<p role="alert" className="operator-caveat">{failure}</p>}
  {message&&<p role="status" className="operator-evidence">{message}</p>}
  {report&&<div className="operator-control-panel"><h4>٣. نتائج الدراسة المحفوظة</h4><p>معامل الارتباط من −1 إلى +1. الأرقام تصف تاريخ المصادر، ولا تتنبأ بحركة الذهب.</p>
   <div className="operator-table-scroll"><table><thead><tr><th>المصدر الأول</th><th>المصدر الثاني</th><th>تداخل بالدقائق</th><th>معامل الارتباط</th><th>حالة الدليل</th></tr></thead>
   <tbody>{pairs.map((pair,i)=><tr key={i}><td>{txt(pair.left)}</td><td>{txt(pair.right)}</td><td>{value(pair.overlap)??'—'}</td><td>{value(pair.correlation)!==null?Number(pair.correlation).toFixed(4):'—'}</td><td>{pair.available===true?'مؤهل حسابياً':txt(pair.reason,'دليل غير كافٍ')}</td></tr>)}
   {!pairs.length&&<tr><td colSpan={5}>لا توجد أزواج موثقة في هذا التقرير.</td></tr>}</tbody></table></div>
  </div>}
  <p className="operator-caveat"><ShieldAlert size={16}/> الارتباط لا يثبت السببية ولا يحوّل نتائج المحاكاة إلى ربح حقيقي. عند فشل التداخل لا نستبدل القيم الناقصة بأرقام مصطنعة.</p>
 </section>;
}
