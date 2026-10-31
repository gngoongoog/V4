import {useCallback,useEffect,useState} from 'react';
import {BarChart3,BookOpen,RefreshCw,ShieldAlert} from 'lucide-react';
import {correlationAction} from './bridge';

type Row=Record<string,unknown>;
const obj=(v:unknown):Row=>v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Row:{};
const arr=(v:unknown):Row[]=>Array.isArray(v)?v.filter(x=>x!==null&&typeof x==='object'&&!Array.isArray(x)):[];
const txt=(v:unknown,alt='—')=>typeof v==='string'&&v.trim()?v:alt;
const value=(v:unknown)=>typeof v==='number'&&Number.isFinite(v)?v:null;
const stamp=(v:unknown)=>value(v)!==null&&Number(v)>0?new Date(Number(v)).toLocaleString('ar-IQ'):'—';
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
   setSources(valid);setReports(saved);setChosen(prev=>prev.filter(id=>valid.some(s=>s.id===id)));
   setFailure('');
  }catch(e){setFailure('تعذر جلب المصادر أو التقارير التاريخية: '+String(e))}
  finally{setLoading(false)}
 },[preview]);
 useEffect(()=>{void refresh()},[refresh]);
 const toggle=(id:string)=>{
  setChosen(current=>current.includes(id)?current.filter(x=>x!==id):current.length<8?[...current,id]:current);
 };
 const analyze=async()=>{
  const n=Number(overlap);
  if(chosen.length<2||chosen.length>8){setFailure('اختر مصدرين إلى ثمانية مصادر محفوظة');return}
  if(!Number.isInteger(n)||n<2||n>60000){setFailure('حد التداخل: عدد صحيح بين 2 و60000 دقيقة');return}
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
   <h4>١. حدد المصادر التاريخية</h4><p>اختَر من 2 إلى 8 نتائج تاريخية محفوظة. يجب أن تتقاطع فتراتها، وقد يرفض Core مصادر غير متوافقة.</p>
   {sources.length?<div className="operator-choose-grid">{sources.map((s,i)=>{
    const id=txt(s.id,'');return <label key={id||String(i)}><input type="checkbox" disabled={!ready||(!chosen.includes(id)&&chosen.length>=8)} checked={chosen.includes(id)} onChange={()=>toggle(id)}/><span><b>{txt(s.symbol,'رمز غير متاح')}</b><small>{id.slice(0,8)} · تاريخ: {value(s.from)!==null?new Date(Number(s.from)*1000).toISOString().slice(0,10):'—'} → {value(s.to)!==null?new Date(Number(s.to)*1000).toISOString().slice(0,10):'—'}</small></span></label>;
   })}</div>:<p className="operator-empty">{loading?'جاري تحميل المصادر...':'لا توجد اختبارات تاريخية محفوظة مؤهلة في Core.'}</p>}
   <div className="operator-filters"><label>التداخل الأدنى بين الشموع بالدقائق<input type="number" aria-label="حد التداخل بالارتباط" min="2" max="60000" step="1" value={overlap} onChange={e=>setOverlap(e.target.value)} disabled={!ready}/></label></div>
   {symbols.length===1&&chosen.length>=2&&<p className="safe-note">المصادر المختارة تخص الرمز نفسه ({symbols[0]})؛ قد تكون نتيجة ارتباط ذاتي لا تفيد تنويع المخاطر.</p>}
   <div className="operator-links"><button type="button" disabled={!ready||chosen.length<2} onClick={()=>void analyze()}>تحليل ارتباط المصادر المختارة</button><button type="button" onClick={onBacktest}><BookOpen size={15}/> إنشاء اختبار تاريخي</button></div>
  </div>
  <div className="operator-control-panel">
   <h4>٢. التقارير المحفوظة</h4>
   {reports.length?<div className="operator-choose-grid">{reports.map((item,i)=>{const s=obj(item.summary),id=txt(s.id,'');return <button key={id||i} className={active===id?'selected':''} disabled={!ready||!id} onClick={()=>void openReport(id)}><b>{(Array.isArray(s.symbols)?s.symbols.filter(x=>typeof x==='string').join(' / '):'')||'ارتباط محفوظ'}</b><small>المعرف {id.slice(0,12)} · {stamp(item.created_ms)} · أزواج: {String(s.pair_count??'—')}</small></button>})}</div>:<p className="operator-empty">لا توجد دراسات ارتباط محفوظة لعرضها.</p>}
  </div>
  {failure&&<p role="alert" className="operator-caveat">{failure}</p>}
  {message&&<p role="status" className="operator-evidence">{message}</p>}
  {report&&<div className="operator-control-panel"><h4>٣. نتائج الدراسة المحفوظة</h4><p>معامل الارتباط من −1 إلى +1. الأرقام تصف تاريخ المصادر، ولا تتنبأ بحركة الذهب.</p>
   <div className="operator-table-scroll"><table><thead><tr><th>المصدر الأول</th><th>المصدر الثاني</th><th>تداخل بالدقائق</th><th>معامل الارتباط</th><th>حالة الدليل</th></tr></thead>
   <tbody>{pairs.map((pair,i)=><tr key={i}><td>{txt(pair.left)}</td><td>{txt(pair.right)}</td><td>{value(pair.overlap)??'—'}</td><td>{value(pair.correlation)!==null?Number(pair.correlation).toFixed(4):'—'}</td><td>{pair.available===true?'مؤهل حسابياً':'عينة غير كافية'}</td></tr>)}
   {!pairs.length&&<tr><td colSpan={5}>لا توجد أزواج موثقة في هذا التقرير.</td></tr>}</tbody></table></div>
  </div>}
  <p className="operator-caveat"><ShieldAlert size={16}/> الارتباط لا يثبت السببية ولا يحوّل نتائج المحاكاة إلى ربح حقيقي. عند فشل التداخل لا نستبدل القيم الناقصة بأرقام مصطنعة.</p>
 </section>;
}
