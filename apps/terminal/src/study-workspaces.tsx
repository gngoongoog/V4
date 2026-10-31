import {useCallback,useEffect,useState} from 'react';
import type {FormEvent,ReactNode} from 'react';
import {BarChart3,BookOpen,CalendarClock,CheckCircle2,Download,Play,RefreshCw,ShieldAlert,Square,TrendingUp} from 'lucide-react';
import {studyAction} from './bridge';
import type {StudyPlan} from './bridge';

type RecordValue=Record<string,unknown>;
const obj=(value:unknown):RecordValue=>value!==null&&typeof value==='object'&&!Array.isArray(value)?value as RecordValue:{};
const arr=(value:unknown):RecordValue[]=>Array.isArray(value)?value.map(obj):[];
const txt=(value:unknown,fallback='—')=>typeof value==='string'&&value.trim()?value:fallback;
const number=(v:unknown)=>typeof v==='number'&&Number.isFinite(v)?v:null;
const show=(v:unknown,digits=2,suffix='')=>number(v)===null?'—':Number(v).toLocaleString('en-US',{maximumFractionDigits:digits,minimumFractionDigits:digits})+suffix;
const date=(days:number)=>new Date(Date.now()+days*86400000).toISOString().slice(0,10);
const running=(state:string)=>state==='loading'||state==='running';
const stage=(value:string)=>({idle:'جاهز',loading:'جلب تاريخ الوسيط',running:'جاري الحساب',done:'اكتمل',completed:'اكتمل',cancelled:'ملغي',error:'فشل'} as Record<string,string>)[value]||value;

function Tip({title,children}:{title:string;children:ReactNode}){
 return <div className="study-tip" dir="rtl"><b>{title}</b><span>{children}</span></div>;
}
function Metric({name,value,hint}:{name:string;value:string;hint:string}){
 return <div className="study-metric" dir="rtl"><small>{name}</small><strong>{value}</strong><span>{hint}</span></div>;
}
function StateBar({source,description}:{source:unknown;description:string}){
 const state=obj(source),phase=txt(state.state,'idle'),p=number(state.progress);
 return <div className="study-state" dir="rtl" role="status">
  <span className={'study-phase '+(running(phase)?'processing':phase==='error'?'failed':'')}>{stage(phase)}</span>
  <div><b>{txt(state.message,description)}</b><small>يُنشئ Core النتائج ويحفظها، ولا تُرسل هذه الأداة أوامر تداول</small></div>
  {p!==null&&running(phase)&&<span className="study-percent">{show(p,0,'%')}</span>}
 </div>;
}
function Warning(){return <div className="study-research-warning" dir="rtl"><ShieldAlert size={19}/><span>هذه نتائج محاكاة/دراسة تاريخية وليست صفقات MT5 منفذة أو دليلاً على الربح المستقبلي. اقرأ التراجع والتكاليف وفجوات التاريخ قبل الحكم على الاستراتيجية.</span></div>}

export function BacktestWorkspace({source,disabled,armed,onStart,onCancel,onRefresh,onEvaluation}:{
 source:unknown;disabled:boolean;armed:boolean;
 onStart:(input:{from_date:string;to_date:string;initial_equity:number;spread_floor:number;strategy:string})=>Promise<void>;
 onCancel:()=>Promise<void>;onRefresh:()=>void;onEvaluation:()=>void;
}){
 const [from,setFrom]=useState(date(-16)),[to,setTo]=useState(date(-1));
 const [equity,setEquity]=useState('1000'),[spread,setSpread]=useState('0.20'),[strategy,setStrategy]=useState('meta_v1');
 const [runs,setRuns]=useState<RecordValue[]>([]),[selected,setSelected]=useState(''),[report,setReport]=useState<RecordValue|null>(null);
 const [busy,setBusy]=useState(false),[loading,setLoading]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState('');
 const state=obj(source),phase=txt(state.state,'idle'),active=running(phase);
 const refresh=useCallback(async()=>{
  if(disabled)return;
  setLoading(true);
  try{
   const response=obj(await studyAction('backtest','runs'));
   setRuns(arr(response.runs));setError('');
  }catch(e){setError('تعذر قراءة الاختبارات المحفوظة: '+String(e))}
  finally{setLoading(false)}
 },[disabled]);
 useEffect(()=>{void refresh()},[refresh,txt(state.latest_id,'')]);
 const openReport=async(id:string)=>{
  if(!id||disabled)return;
  setBusy(true);setReport(null);setError('');
  try{const response=obj(await studyAction('backtest','report',id));setReport(response);setSelected(id)}
  catch(e){setError('تعذر قراءة نتيجة الاختبار: '+String(e))}
  finally{setBusy(false)}
 };
 const submit=async(e:FormEvent)=>{
  e.preventDefault();
  const amount=Number(equity),floor=Number(spread);
  if(from>=to||to>date(0)||!Number.isFinite(amount)||amount<=0||!Number.isFinite(floor)||floor<0){
   setError('الفترة تحتاج بداية قبل النهاية، ورصيداً موجباً، وسبريداً لا يقل عن الصفر');return;
  }
  if(!['meta_v1','meta_v2','ces_v1','kte_v1','explore_v1'].includes(strategy)){setError('استراتيجية غير مدعومة');return}
  setBusy(true);setError('');setNotice('');
  try{await onStart({from_date:from,to_date:to,initial_equity:amount,spread_floor:floor,strategy});setNotice('أُرسل طلب الاختبار إلى Core. راقب حالة الجلب والحساب أعلاه.')}
  catch(e){setError(String(e))}
  finally{setBusy(false)}
 };
 const exportReport=async()=>{
  if(!selected)return;setBusy(true);
  try{const result=obj(await studyAction('backtest','export',selected));setNotice('حفظ Core ملف التصدير في مجلد بياناته: '+txt(result.path,txt(result.directory,'راجع مجلد exports')))}
  catch(e){setError(String(e))}finally{setBusy(false)}
 };
 const metrics=obj(report?.metrics);
 const outcome=report!==null;
 return <section className="study-workspace" dir="rtl" aria-label="Backtest workspace">
  <div className="study-hero"><div><small>STRATEGY LAB • HISTORICAL</small><h2><CalendarClock size={24}/> مختبر الاختبار التاريخي</h2><p>اختَر الاستراتيجية والفترة، شغّل المحاكاة على شموع MT5 المكتملة، وبعدها افتح تقريراً واضحاً للنتائج والمخاطر.</p></div><button type="button" onClick={()=>{onRefresh();void refresh()}} disabled={disabled||loading}><RefreshCw size={16}/> تحديث البيانات</button></div>
  <div className="study-tips"><Tip title="١. جهّز الفترة">يحتاج Core إلى حساب MT5 حديث، تاريخ M1 كافٍ، وإيقاف START. النهاية غير مشمولة بالتوقيت العالمي UTC.</Tip><Tip title="٢. شغّل الاختبار">يحسب Core الصفقات الافتراضية والتكاليف والانزلاق وفق إعدادات المخاطر التاريخية. لا يفتح صفقة حقيقية.</Tip><Tip title="٣. راجع النتائج">قارن صافي الربح، حجم العينة، السحب الأقصى، تكلفة العمولة، والفجوات قبل التفكير بأي تعديل.</Tip></div>
  <StateBar source={state} description="لا توجد مهمة حالية. يمكنك مراجعة أي تقرير سابق."/>
  <div className="study-two-column">
   <form className="operator-card study-form" onSubmit={e=>void submit(e)}>
    <h3><Play size={18}/> إعداد تجربة جديدة</h3>
    <label>الاستراتيجية
     <select value={strategy} onChange={e=>setStrategy(e.target.value)} disabled={disabled||active||busy} aria-label="الاستراتيجية التاريخية">
      <option value="meta_v1">Meta V1 — الاستراتيجية الأساسية</option>
      <option value="meta_v2">Meta V2 — التحليل المرشّح</option>
      <option value="ces_v1">CES — الانتروبيا الحرجة</option>
      <option value="kte_v1">KTE — Kinetic Time Echo</option>
      <option value="explore_v1">Explore — استكشاف بحثي فقط</option>
     </select>
    </label>
    <div className="study-fields">
     <label>من يوم (UTC)<input type="date" aria-label="backtest from" value={from} max={to} required onChange={e=>setFrom(e.target.value)}/></label>
     <label>إلى يوم، غير شامل (UTC)<input type="date" aria-label="backtest to" value={to} max={date(0)} required onChange={e=>setTo(e.target.value)}/></label>
     <label>رأس المال الافتراضي<input type="number" aria-label="initial equity" min="1" step="any" value={equity} onChange={e=>setEquity(e.target.value)}/></label>
     <label>أرضية السبريد بسعر الذهب<input type="number" aria-label="spread floor" min="0" step="0.01" value={spread} onChange={e=>setSpread(e.target.value)}/></label>
    </div>
    <small className="study-help">السبريد هنا قيمة سعرية دنيا للمحاكاة، وليس عدد نقاط MT5. النتائج تقديرية، والعمولة تتبع إعداد المخاطر في Core.</small>
    <div className="study-actions"><button type="submit" className="study-primary" disabled={disabled||armed||active||busy}><Play size={16}/> بدء الاختبار</button><button type="button" disabled={disabled||!active||busy} onClick={()=>void onCancel()}><Square size={16}/> إلغاء المهمة</button></div>
    {armed&&<p className="safe-note">اضغط STOP قبل جلب اختبار جديد. التقارير القديمة متاحة دون تشغيل تداول.</p>}
   </form>
   <div className="operator-card study-history"><div className="study-row"><h3><BookOpen size={18}/> الاختبارات المحفوظة</h3><button type="button" onClick={()=>void refresh()} disabled={disabled||loading}><RefreshCw size={15}/> تحديث</button></div>
    {!runs.length?<p className="study-empty">لا توجد اختبارات تاريخية محفوظة في Core الحالي. شغّل أول تجربة من الاستمارة.</p>:
    <div className="study-run-list">{runs.map(run=><button type="button" key={txt(run.id)} className={selected===run.id?'chosen':''} disabled={busy||disabled} onClick={()=>void openReport(String(run.id))}><b>{txt(run.label,txt(run.algorithm,'نتيجة تاريخية'))}</b><small>رقم النتيجة: {txt(run.id).slice(0,8)}</small></button>)}</div>}
   </div>
  </div>
  {error&&<div className="operator-error" role="alert">{error}</div>}
  {notice&&<p className="study-notice" role="status">{notice}</p>}
  {outcome?<div className="operator-card study-results">
    <div className="study-row"><div><small>تقرير محفوظ من Core</small><h3><TrendingUp size={18}/> قراءة نتيجة المحاكاة</h3></div><div className="study-actions"><button type="button" onClick={()=>void exportReport()} disabled={busy||disabled}><Download size={15}/> تصدير التقرير</button><button type="button" onClick={onEvaluation}>تقييم خارج العينة</button></div></div>
    <div className="study-metrics"><Metric name="صافي النتيجة" value={show(metrics.net,2)} hint="بعد التكاليف المحاكية"/><Metric name="عدد الصفقات" value={show(metrics.trades,0)} hint="داخل التاريخ المختبر"/><Metric name="نسبة الفوز" value={show(metrics.win_rate,1,'%')} hint="لا تكفي وحدها للحكم"/><Metric name="Profit Factor" value={show(metrics.profit_factor)} hint="غير معرّف دون خسائر"/><Metric name="أقصى تراجع" value={show(metrics.max_drawdown_percent,2,'%')} hint="داخل العينة المختبرة"/><Metric name="العمولات" value={show(metrics.commissions,2)} hint="محسوبة تقديرياً"/></div>
    <details className="study-raw"><summary>التقرير التفصيلي وأسباب الدخول والخروج</summary><pre dir="auto">{txt(report?.text,txt(report?.summary,'لا توجد تفاصيل أخرى'))}</pre></details>
  </div>:<div className="study-empty study-prompt">اختر اختباراً محفوظاً من القائمة لتظهر هنا قراءة مفهومة للنتائج، بدلاً من عرض JSON خام.</div>}
  <Warning/>
 </section>;
}

export function EvaluationWorkspace({source,disabled,onRefresh,onBacktest}:{
 source:unknown;disabled:boolean;onRefresh:()=>void;onBacktest:()=>void;
}){
 const state=obj(source),phase=txt(state.state,'idle');
 const [sources,setSources]=useState<RecordValue[]>([]),[runs,setRuns]=useState<RecordValue[]>([]);
 const [sourceId,setSourceId]=useState(''),[mode,setMode]=useState<'holdout'|'rolling'>('holdout');
 const [holdout,setHoldout]=useState('30'),[development,setDevelopment]=useState('7'),[holdDays,setHoldDays]=useState('2'),[step,setStep]=useState('2');
 const [selected,setSelected]=useState(''),[report,setReport]=useState<RecordValue|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState('');
 const refresh=useCallback(async()=>{
  if(disabled)return;
  try{
   const [a,b]=await Promise.all([studyAction('backtest','runs'),studyAction('evaluation','runs')]);
   setSources(arr(obj(a).runs).filter(r=>/^meta-v(?:1|2)/i.test(txt(r.algorithm,''))));setRuns(arr(obj(b).runs));setError('');
  }catch(e){setError('تعذر قراءة نتائج Core المحفوظة: '+String(e))}
 },[disabled]);
 useEffect(()=>{void refresh()},[refresh,txt(state.latest_id,'')]);
 const chosen=sourceId||String(sources[0]?.id||'');
 const submit=async(e:FormEvent)=>{
  e.preventDefault();if(!chosen){setError('ابدأ باختبار تاريخي محفوظ، ثم اختر نتيجته هنا');return}
  const n=Number(holdout),d=Number(development),h=Number(holdDays),st=Number(step);
  if(mode==='holdout'&&(!Number.isInteger(n)||n<10||n>50)){setError('Holdout يجب أن يكون بين 10% و50%');return}
  if(mode==='rolling'&&(![d,h,st].every(Number.isInteger)||d<2||h<1||st<1)){setError('Rolling: التطوير يومان على الأقل، والاختبار والخطوة يوم على الأقل');return}
  const plan:StudyPlan=mode==='holdout'?{mode,holdout_percent:n}:{mode,development_days:d,holdout_days:h,step_days:st};
  setBusy(true);setError('');setNotice('');
  try{await studyAction('evaluation','start',undefined,chosen,plan);onRefresh();setNotice('بدأ Core مقارنة Meta V1 مع Meta V2 زمنياً من النتيجة المحفوظة، بدون تداول.')}
  catch(e){setError(String(e))}finally{setBusy(false)}
 };
 const openReport=async(id:string)=>{
  if(!id)return;setBusy(true);setError('');setReport(null);
  try{setReport(obj(await studyAction('evaluation','report',id)));setSelected(id)}
  catch(e){setError(String(e))}finally{setBusy(false)}
 };
 const cancel=async()=>{setBusy(true);try{await studyAction('evaluation','cancel');onRefresh();setNotice('أُرسل طلب إلغاء التقييم إلى Core')}catch(e){setError(String(e))}finally{setBusy(false)}};
 const exportReport=async()=>{setBusy(true);try{const r=obj(await studyAction('evaluation','export',selected));setNotice('تم التصدير داخل Core: '+txt(r.path,'راجع مجلد exports'))}catch(e){setError(String(e))}finally{setBusy(false)}};
 const evaluation=obj(report?.evaluation),v1=obj(evaluation.meta_v1),v2=obj(evaluation.meta_v2);
 return <section className="study-workspace" dir="rtl" aria-label="Strategy evaluation workspace">
  <div className="study-hero"><div><small>WALK-FORWARD • OUT-OF-SAMPLE</small><h2><BarChart3 size={24}/> مختبر تقييم الاستراتيجيات</h2><p>لا تعتمد على اختبار واحد. افصل زمن التطوير عن زمن الاختبار، ثم قارن Meta V1 وMeta V2 على نوافذ مستقلة.</p></div><button type="button" disabled={disabled} onClick={()=>{onRefresh();void refresh()}}><RefreshCw size={16}/> تحديث النتائج</button></div>
  <div className="study-tips"><Tip title="١. نتيجة أصلية">ابدأ باختبار تاريخي محفوظ لاستراتيجية Meta V1 أو Meta V2. لا تحتاج فتح MT5 لقراءة النتائج السابقة.</Tip><Tip title="٢. Holdout">احجز آخر جزء من التاريخ للاختبار؛ لا تعدّل الاستراتيجية بناءً على نتيجة الاختبار نفسها.</Tip><Tip title="٣. Rolling">تتكرر نافذة التطوير والاختبار عبر الزمن، فتظهر درجة ثبات النتائج لا أفضل نتيجة فقط.</Tip></div>
  <StateBar source={state} description="لا يوجد تقييم نشط. اختر نتيجة محفوظة وخطة زمنية."/>
  <div className="study-two-column">
   <form className="operator-card study-form" onSubmit={e=>void submit(e)}>
    <h3><CalendarClock size={18}/> إعداد تقييم جديد</h3>
    <label>مصدر التاريخ المحفوظ
      <select aria-label="اختيار مصدر الاختبار للتقييم" disabled={disabled||busy||running(phase)||!sources.length} value={chosen} onChange={e=>setSourceId(e.target.value)}>
       {!sources.length&&<option value="">لا توجد نتائج Meta V1/V2 مؤهلة</option>}
       {sources.map(r=><option key={txt(r.id)} value={txt(r.id)}>{txt(r.label,txt(r.algorithm,'نتيجة تاريخية'))}</option>)}
      </select>
    </label>
    <label>طريقة اختبار الثبات
     <select aria-label="خطة التقييم" value={mode} onChange={e=>setMode(e.target.value==='rolling'?'rolling':'holdout')} disabled={disabled||busy||running(phase)}>
      <option value="holdout">Holdout — فصل الفترة إلى تطوير واختبار</option><option value="rolling">Rolling — اختبار متكرر عبر الزمن</option>
     </select>
    </label>
    {mode==='holdout'?<label>نسبة التاريخ المحجوز للاختبار (%)<input type="number" min="10" max="50" step="1" value={holdout} onChange={e=>setHoldout(e.target.value)}/></label>:
    <div className="study-fields"><label>أيام التطوير<input type="number" min="2" max="365" value={development} onChange={e=>setDevelopment(e.target.value)}/></label><label>أيام الاختبار<input type="number" min="1" max="365" value={holdDays} onChange={e=>setHoldDays(e.target.value)}/></label><label>خطوة النافذة بالأيام<input type="number" min="1" max="365" value={step} onChange={e=>setStep(e.target.value)}/></label></div>}
    <div className="study-actions"><button type="submit" className="study-primary" disabled={disabled||busy||running(phase)||!sources.length}><Play size={16}/> بدء التقييم الزمني</button><button type="button" disabled={disabled||busy||!running(phase)} onClick={()=>void cancel()}><Square size={16}/> إلغاء التقييم</button></div>
    <p className="safe-note">تدعم هذه الطبقة Meta V1 وMeta V2 فقط، ولا تغيّر العقل الذي ينفذ التداول. لا ترقّي أي نموذج تلقائياً.</p>
    <button type="button" onClick={onBacktest}><BookOpen size={15}/> انتقل لإنشاء اختبار تاريخي</button>
   </form>
   <div className="operator-card study-history"><div className="study-row"><h3><BookOpen size={18}/> التقييمات المحفوظة</h3><button type="button" onClick={()=>void refresh()} disabled={disabled||busy}><RefreshCw size={15}/> تحديث</button></div>
    {!runs.length?<p className="study-empty">ماكو تقييمات محفوظة حتى الآن. أنشئ تقييم Holdout أو Rolling من البيانات التاريخية.</p>:<div className="study-run-list">{runs.map(r=><button type="button" key={txt(r.id)} className={selected===r.id?'chosen':''} disabled={disabled||busy} onClick={()=>void openReport(String(r.id))}><b>{txt(r.symbol,'XAUUSD')} · {show(r.windows,0)} نوافذ</b><small>Meta V1: {show(r.meta_v1_net_sum)} · Meta V2: {show(r.meta_v2_net_sum)}</small></button>)}</div>}
   </div>
  </div>
  {error&&<div role="alert" className="operator-error">{error}</div>}
  {notice&&<div role="status" className="study-notice">{notice}</div>}
  {report?<div className="operator-card study-results">
   <div className="study-row"><h3><CheckCircle2 size={18}/> مقارنة الأداء عبر الفترات</h3><button disabled={disabled||busy} onClick={()=>void exportReport()}><Download size={15}/> تصدير أدلة التقييم</button></div>
   <div className="study-compare"><div><h4>Meta V1 — خط الأساس</h4><Metric name="صافي النتائج عبر النوافذ" value={show(v1.net_sum)} hint="جمع وصفي، لا رأس مال متصل"/><Metric name="عدد الصفقات" value={show(v1.trades,0)} hint="من نوافذ التقييم"/><Metric name="أسوأ تراجع" value={show(v1.worst_drawdown_percent,2,'%')} hint="داخل نافذة مستقلة"/></div><div><h4>Meta V2 — المرشح</h4><Metric name="صافي النتائج عبر النوافذ" value={show(v2.net_sum)} hint="لا يثبت أفضلية مستقبلية"/><Metric name="عدد الصفقات" value={show(v2.trades,0)} hint="من نوافذ التقييم"/><Metric name="أسوأ تراجع" value={show(v2.worst_drawdown_percent,2,'%')} hint="داخل نافذة مستقلة"/></div></div>
   <p className="study-notice">الفارق المتوسط V2 − V1 لكل نافذة: {show(evaluation.average_delta_net_v2_minus_v1)}. الموجب لا يعني أنه مضمون أو يصلح للتشغيل الحقيقي.</p>
   <details className="study-raw"><summary>تفاصيل خطة التقييم وملاحظات Core</summary><pre dir="auto">{JSON.stringify({plan:evaluation.plan,note:evaluation.note,windows:arr(evaluation.windows).map(w=>({window:obj(w.window),meta_v1:obj(obj(w.meta_v1).metrics),meta_v2:obj(obj(w.meta_v2).metrics)}))},null,2)}</pre></details>
  </div>:<div className="study-empty study-prompt">اختر تقييماً محفوظاً لمقارنة نتائج Meta V1 وMeta V2 جنباً إلى جنب.</div>}
  <Warning/>
 </section>;
}
