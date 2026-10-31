import './operator-pages.css';
import {useMemo,useState} from 'react';
import {Activity,BarChart3,BookOpen,ExternalLink,Filter,Layers3,LineChart,Newspaper,RefreshCw,Search,ShieldAlert} from 'lucide-react';
import type {Position} from './types';

type Row=Record<string,unknown>;
type Page='news'|'studio'|'correlation'|'analysis'|'shadow'|'events';
const object=(value:unknown):Row=>value!==null&&typeof value==='object'&&!Array.isArray(value)?value as Row:{};
const rows=(value:unknown):Row[]=>Array.isArray(value)?value.filter(x=>x!==null&&typeof x==='object'&&!Array.isArray(x)) as Row[]:[];
const txt=(v:unknown,empty='—')=>typeof v==='string'&&v.trim()?v:empty;
const num=(v:unknown)=>typeof v==='number'&&Number.isFinite(v)?v:null;
const count=(v:unknown)=>num(v)===null?'—':Number(v).toLocaleString('en-US');
const stamp=(v:unknown)=>num(v)!==null&&Number(v)>0&&Number(v)<8.64e15?new Date(Number(v)).toLocaleString('ar-IQ',{dateStyle:'short',timeStyle:'short'}):'غير متاح';
const authorizedArticle=(v:unknown):string|null=>{
 if(typeof v!=='string')return null;
 try{
  const u=new URL(v);
  return u.protocol==='https:'&&['www.federalreserve.gov','federalreserve.gov','www.ecb.europa.eu','ecb.europa.eu'].includes(u.hostname)&&!u.username&&!u.password?u.href:null;
 }catch{return null}
};
function Evidence({source,preview,error}:{source:unknown;preview:boolean;error:string}){
 return <p className="operator-evidence" role="status">{preview?'معاينة تصميم: لا بيانات مصدر موثقة':error?'خطأ قراءة Core: '+error:source===null?'بانتظار رد Core...':'البيانات التالية من استجابة Core الحالية، وليست إثبات تنفيذ صفقة بالوسيط'}</p>;
}
function Measure({label,value,detail}:{label:string;value:string;detail:string}){
 return <div className="operator-measure"><small>{label}</small><b>{value}</b><span>{detail}</span></div>;
}
function ReadonlyNotice({children}:{children:string}){return <p className="operator-caveat"><ShieldAlert size={16}/>{children}</p>}
export function NewsOperatorWorkspace({source,preview,error,disabled,onRefresh,onAction}:{source:unknown;preview:boolean;error:string;disabled:boolean;onRefresh:()=>void;onAction:(action:'refresh'|'enable'|'disable')=>Promise<void>}){
 const data=object(source),feeds=rows(data.sources),headlines=rows(data.items);
 const [term,setTerm]=useState(''),[feed,setFeed]=useState('all'),[freshOnly,setFreshOnly]=useState(false);
 const [actionBusy,setActionBusy]=useState(false),[actionError,setActionError]=useState(''),[actionSuccess,setActionSuccess]=useState('');
 const act=async(action:'refresh'|'enable'|'disable')=>{
  if(preview||disabled||actionBusy)return;
  setActionBusy(true);setActionError('');setActionSuccess('');
  try{
   await onAction(action);
   setActionSuccess(action==='refresh'?'Core قبل طلب تحديث المصادر؛ البيانات الفعلية تتحدث لاحقاً.':'أكد Core حفظ إعداد مصادر الأخبار. يمكن مراجعة الحالة أعلاه.');
  }catch(e){setActionError('تعذر تأكيد عملية الأخبار: '+String(e))}
  finally{setActionBusy(false)}
 };
 const visible=useMemo(()=>headlines.filter(item=>(feed==='all'||item.source===feed)&&
  (!freshOnly||item.source_stale===false)&&
  (!term||[item.title,item.topic,item.source].some(v=>String(v||'').toLowerCase().includes(term.toLowerCase())))),[headlines,feed,freshOnly,term]);
 const macro=object(data.macro);
 return <section className="operator-workspace" dir="rtl" aria-label="غرفة أخبار السوق">
  <div className="operator-workspace-head"><div><h3><Newspaper size={20}/> مصادر أخبار الذهب والاقتصاد</h3><p>عناوين منشورة من مصادر رسمية فقط. هذه الصفحة ليست تقويم أخبار مستقبلية ولا تصدر إشارة شراء أو بيع.</p></div><button onClick={onRefresh} disabled={preview}><RefreshCw size={16}/> تحديث قراءة Core</button></div>
  <div className="operator-links">
   <button type="button" disabled={preview||disabled||actionBusy||data.offline===true||data.enabled!==true||data.refreshing===true} onClick={()=>void act('refresh')}><RefreshCw size={15}/> جلب الأخبار الآن من المصادر</button>
   <button type="button" disabled={preview||disabled||actionBusy||data.enabled===undefined} onClick={()=>void act(data.enabled===true?'disable':'enable')}>{data.enabled===true?'إيقاف تحديث المصادر':'تفعيل تحديث المصادر'}</button>
  </div>
  {actionError&&<p className="operator-caveat" role="alert">{actionError}</p>}
  {actionSuccess&&<p className="operator-evidence" role="status">{actionSuccess}</p>}
  <Evidence source={source} preview={preview} error={error}/>
  {!preview&&!error&&source!==null&&<>
   <div className="operator-measures">
    <Measure label="المصادر الحديثة" value={count(macro.fresh_sources)} detail="بحسب Core"/>
    <Measure label="الأخبار بآخر ساعة" value={count(macro.recent_60m)} detail="أخبار منشورة، ليست توقعات"/>
    <Measure label="حالة Macro" value={txt(macro.state,'غير معروفة')} detail={macro.current===true?'سياق حديث':'السياق غير مؤكد أو قديم'}/>
    <Measure label="التحديث الآلي" value={data.enabled===true?'مفعّل':data.enabled===false?'متوقف':'غير معروف'} detail={data.refreshing===true?'جاري تحديث المصادر':data.offline===true?'المحرك دون إنترنت':'وفق حالة المحرك'}/>
   </div>
   <h4>سلامة مصادر الأخبار</h4>
   <div className="operator-source-grid">{feeds.length?feeds.map((s,i)=><article key={txt(s.id,String(i))}><b>{txt(s.name)}</b><span>{s.stale===false?'مصدر حديث':'بيانات قديمة / غير متاحة'}</span><small>آخر تحديث ناجح: {stamp(s.last_success_ms)}</small><small>عناوين مخزنة: {count(s.items)}</small>{txt(s.error,'')&&<p role="alert">{txt(s.error)}</p>}</article>):<p>لم يرجع Core تفاصيل المصادر.</p>}</div>
   {txt(data.storage_error,'')&&<p role="alert" className="operator-caveat">خطأ تخزين الأخبار: {txt(data.storage_error)}</p>}
   <h4>تصفّح الأخبار المنشورة</h4>
   <div className="operator-filters"><label><Search size={16}/> بحث<input type="search" aria-label="بحث بالأخبار" value={term} onChange={e=>setTerm(e.target.value)} placeholder="العنوان، الموضوع، المصدر"/></label>
    <label><Filter size={16}/> المصدر<select aria-label="مصدر الأخبار" value={feed} onChange={e=>setFeed(e.target.value)}><option value="all">كل المصادر</option>{feeds.map((s,i)=><option key={i} value={txt(s.name)}>{txt(s.name)}</option>)}</select></label>
    <label className="operator-check"><input type="checkbox" checked={freshOnly} onChange={e=>setFreshOnly(e.target.checked)}/> مصادر حديثة فقط</label>
   </div>
   <small>يظهر {visible.length} من {headlines.length} عنوان في رد Core، والبحث داخل النتائج المعروضة فقط.</small>
   <div className="operator-news-list">{visible.length?visible.slice(0,60).map((n,i)=>{
    const url=authorizedArticle(n.url);
    return <article key={txt(n.url,String(i))}><div><b>{txt(n.title,'عنوان غير متاح')}</b><small>{txt(n.source)} · {txt(n.topic,'غير مصنّف')} · {stamp(n.published_ms)}</small><span>{n.source_stale===false?'من مصدر حديث':'المصدر قديم أو تعذر التحقق من تحديثه'}</span></div>
     {url?<a href={url} target="_blank" rel="noopener noreferrer" aria-label={'افتح الخبر الرسمي '+txt(n.title)}><ExternalLink size={15}/> الخبر الأصلي</a>:<span>الرابط غير موثوق / غير متاح</span>}
    </article>;
   }):<p className="operator-empty">لا توجد أخبار مطابقة. افحص حالة المصدر أو أزل المرشحات.</p>}</div>
  </>}
  <ReadonlyNotice>تصنيف الخبر بالكلمات وسياق Macro ليس قياساً موثوقاً لتأثيره على سعر الذهب ولا يعني جودة أي إشارة تداول.</ReadonlyNotice>
 </section>;
}
const DESCRIPTIONS:Record<Exclude<Page,'news'>,{title:string;description:string;notice:string}> = {
 studio:{title:'استوديو الاستراتيجيات',description:'تغطية فريمات MT5، سجل التجارب وقياسات Wyckoff المستقبلية من Core.',notice:'التقييم المستقبلي داخل الاستوديو بحثي وليس صفقات وسيط أو دليلاً على ربحية متوقعة.'},
 correlation:{title:'مختبر الارتباطات',description:'ملخص تحليل ارتباط الاختبارات التاريخية المحفوظة. لا يتصل بعمق السوق الفوري.',notice:'الارتباط الإحصائي لا يثبت السببية ولا يحدد فرصة تداول؛ النتائج بحثية فقط.'},
 analysis:{title:'سجل فرص التحليل',description:'إعدادات الفرص التي سُجل أول ظهور لها. لا تعكس سعراً حياً أو إشارات دخول آنية.',notice:'تسجيل فرصة بحثية ليس تنفيذ صفقة ولا توصية دخول حالية.'},
 shadow:{title:'سجل قرارات الظل',description:'قرارات تحليلية محفوظة؛ تُعرض بوصفها أحداث بحثية قديمة لا تداولاً منفذاً.',notice:'لا تفترض أن قرار الظل أغلق أو فتح مركزاً حقيقياً.'},
 events:{title:'أحداث وتشخيص النظام',description:'آخر الأحداث المسجلة بواسطة Rust Core، مع ترتيبها والتفاصيل الأصلية.',notice:'عدم وجود أحداث لا يعني ضمان سلامة المنظومة؛ حالة الوسيط وGateway تحتاج فحوصاً منفصلة.'}
};
// Read the actual research schemas. No inferred confidence or broker-executed trades.
const moneyValue=(v:unknown)=>num(v)===null?'غير مسجل':Number(v).toLocaleString('en-US',{maximumFractionDigits:4});
const direction=(v:unknown)=>v===1?'شراء افتراضي':v===-1?'بيع افتراضي':'غير محدد';
const eventLabel=(kind:unknown)=>{
 const known:Record<string,string>={gateway_connection:'تغير اتصال MT5',core_start:'بدء المحرك',shutdown:'طلب إيقاف',gateway_error:'خطأ جسر MT5',execution_error:'خطأ التنفيذ',config_updated:'تغيير إعدادات'};
 const label=txt(kind,'حدث غير مصنف');
 return known[label]||label;
};
function EventDetail({page,item}:{page:'analysis'|'shadow'|'events';item:Row}){
 const record=page==='shadow'?object(item.decision):page==='analysis'?object(item.setup):object(item.detail);
 const analysis=object(record.analysis);
 const gates=page==='analysis'?Array.isArray(record.gates)?record.gates:[]:Array.isArray(analysis.gates)?analysis.gates:[];
 const reasons=page==='analysis'?Array.isArray(record.reasons)?record.reasons:[]:[];
 const row=(label:string,value:unknown)=><div className="operator-plain-row" key={label}><span>{label}</span><b>{value===undefined||value===null||value===''?'غير مسجل':String(value)}</b></div>;
 if(page==='analysis')return <div className="operator-event-reading">
  {row('الفرصة / النمط',txt(record.name,txt(record.pattern,'غير مسمى')))}
  {row('الاتجاه المقترح',direction(record.side))}
  {row('حالة التأهيل البحثي',txt(record.state,'غير موثقة'))}
  {row('منطقة السعر',num(record.zone_low)!==null&&num(record.zone_high)!==null?moneyValue(record.zone_low)+' — '+moneyValue(record.zone_high):'غير متاحة')}
  {row('الدخول المسجل',moneyValue(record.entry))}
  {row('وقف الخسارة المسجل',moneyValue(record.stop))}
  {row('الأهداف',Array.isArray(record.targets)&&record.targets.length?record.targets.filter(x=>num(x)!==null).map(moneyValue).join(' / '):'غير مسجلة')}
  {row('نسبة العائد/المخاطرة بعد التكاليف التقديرية',moneyValue(record.net_rr))}
  {reasons.length>0&&<p>ملاحظات الدراسة: {reasons.slice(0,5).map(String).join('؛ ')}</p>}
  {gates.length>0&&<p>أسباب عدم الجاهزية أو القيود: {gates.slice(0,5).map(String).join('؛ ')}</p>}
  <small>هذه لقطة لأول ظهور للفرصة وليست سعراً مباشراً. الأسعار قد تكون غائبة أو منتهية الصلاحية.</small>
 </div>;
 if(page==='shadow')return <div className="operator-event-reading">
  {row('نتيجة قرار البحث',txt(analysis.action,'لا يوجد قرار'))}
  {row('التفسير المسجل',txt(analysis.reason,'لم يرجع Core تفسيراً'))}
  {row('حالة السوق وفق التحليل',txt(analysis.regime,'غير محددة'))}
  {row('سعر Bid وقت تسجيل القرار',moneyValue(record.bid))}
  {row('سعر Ask وقت تسجيل القرار',moneyValue(record.ask))}
  {row('سعر دخول افتراضي',moneyValue(analysis.entry))}
  {row('وقف افتراضي',moneyValue(analysis.stop))}
  {row('هدف افتراضي',moneyValue(analysis.target))}
  {gates.length>0&&<p>شروط حالت دون اتخاذ القرار: {gates.slice(0,5).map(String).join('؛ ')}</p>}
  <small>سجل قرار بحثي، وليس دليلاً على إرسال أو قبول صفقة MT5.</small>
 </div>;
 return <div className="operator-event-reading">
  {row('نوع الحدث',eventLabel(item.kind))}
  {row('معرّف السجل',item.id)}
  {Object.prototype.hasOwnProperty.call(record,'connected')&&row('اتصال Gateway',record.connected===true?'متصل':record.connected===false?'منقطع':'غير مؤكد')}
  {typeof record.state==='string'&&row('الحالة',record.state)}
  {typeof record.reason==='string'&&row('السبب',record.reason)}
  {typeof record.error==='string'&&row('الخطأ المبلّغ',record.error)}
  {typeof record.mode==='string'&&row('وضع التشغيل',record.mode)}
  <small>قد يحتوي الحدث حقولاً تقنية أخرى؛ تعرضها أداة بيانات Core المتقدمة في الأسفل عند الحاجة للتشخيص.</small>
 </div>;
}
export function CoreReadoutWorkspace({page,source,preview,error,disabled,onRefresh,onNavigate,onStudioAction}:{page:Exclude<Page,'news'>;source:unknown;preview:boolean;error:string;disabled:boolean;onRefresh:()=>void;onNavigate:(page:'backtest'|'evaluation'|'execution'|'brainControl'|'events')=>void;onStudioAction:(action:'run'|'auto_enable'|'auto_disable'|'cancel',timeframe?:string)=>Promise<unknown>}){
 const data=object(source),desc=DESCRIPTIONS[page];
 const [search,setSearch]=useState('');
 const [studioFrame,setStudioFrame]=useState('ALL'),[studioBusy,setStudioBusy]=useState(false);
 const [studioError,setStudioError]=useState(''),[studioNotice,setStudioNotice]=useState('');
 const actStudio=async(action:'run'|'auto_enable'|'auto_disable'|'cancel')=>{
  if(preview||disabled||studioBusy)return;
  setStudioBusy(true);setStudioError('');setStudioNotice('');
  try{
   const response=object(await onStudioAction(action,action==='run'?studioFrame:undefined));
   if(action==='run'){
    const queued=num(response.queued);
    setStudioNotice(queued===0?'لم يضع Core أي تجربة جديدة بالطابور. راجع كفاية الشموع والتجارب المعلقة.':'أكد Core جدولة '+count(queued)+' تجربة بحثية؛ هذا ليس تنفيذ تداول.');
   }else setStudioNotice('أكد Core استلام الإجراء البحثي، وتم تحديث حالة الاستوديو.');
  }catch(e){setStudioError('تعذر تأكيد إجراء الاستوديو: '+String(e))}
  finally{setStudioBusy(false)}
 };
 const [frameOnly,setFrameOnly]=useState(false);
 const frames=rows(data.frames),reports=rows(data.reports),learning=rows(data.wyckoff_learning);
 const entries=page==='analysis'?rows(data.setups):page==='shadow'?rows(data.decisions):page==='events'?rows(data.events):[];
 const filtered=entries.filter(x=>!search||JSON.stringify(x).toLowerCase().includes(search.toLowerCase()));
 const present=source!==null&&!preview&&!error;
 return <section className="operator-workspace" dir="rtl" aria-label={desc.title}>
  <div className="operator-workspace-head"><div><h3><Activity size={20}/>{desc.title}</h3><p>{desc.description}</p></div><button disabled={preview} onClick={onRefresh}><RefreshCw size={16}/> تحديث البيانات</button></div>
  <Evidence source={source} preview={preview} error={error}/>
  {present&&<>
   {page==='studio'&&<>
    <div className="operator-measures"><Measure label="حالة الربط" value={data.connected===true?'متصل':data.connected===false?'غير متصل':'غير معروف'} detail="بحسب تقرير الاستوديو"/><Measure label="طابور التجارب" value={count(data.queued)} detail="الطلبات المنتظرة"/><Measure label="تجارب محفوظة" value={String(reports.length)} detail="في استجابة Core الحالية"/><Measure label="التعلّم" value={data.learning_frozen===true?'مجمّد':data.learning_frozen===false?'غير مجمّد':'غير مؤكد'} detail={data.auto===true?'التشغيل التلقائي مفعل':data.auto===false?'التلقائي متوقف':'وضع التلقائي غير معروف'}/></div>
    <div className="operator-control-panel">
     <h4>أوامر الاستوديو البحثي</h4>
     <p>التجارب تتطلب تاريخ MT5 كافياً. أوامر الاستوديو لا تنفذ صفقات ولا تختار نموذجاً للتداول تلقائياً.</p>
     <div className="operator-filters"><label>الفريم<select aria-label="فريم تجربة الاستوديو" disabled={disabled||studioBusy} value={studioFrame} onChange={e=>setStudioFrame(e.target.value)}><option value="ALL">جميع الفريمات المتاحة</option>{frames.map((f,i)=><option key={i} value={txt(f.timeframe)}>{txt(f.timeframe)}</option>)}</select></label></div>
     <div className="operator-links">
      <button type="button" disabled={preview||disabled||studioBusy||data.connected!==true||data.learning_frozen===true} onClick={()=>void actStudio('run')}>تشغيل تجربة بحثية</button>
      <button type="button" disabled={preview||disabled||studioBusy||data.learning_frozen===true||data.auto!==false} onClick={()=>void actStudio('auto_enable')}>تفعيل التشغيل البحثي التلقائي</button>
      <button type="button" disabled={preview||disabled||studioBusy||data.auto!==true} onClick={()=>void actStudio('auto_disable')}>إيقاف التلقائي</button>
      <button type="button" disabled={preview||disabled||studioBusy||(!data.working&&data.auto!==true&&num(data.queued)===0)} onClick={()=>void actStudio('cancel')}>إلغاء مهام الاستوديو</button>
     </div>
     {studioError&&<p role="alert" className="operator-caveat">{studioError}</p>}
     {studioNotice&&<p role="status" className="operator-evidence">{studioNotice}</p>}
    </div>
    <h4>تغطية الأطر الزمنية</h4><div className="operator-filters"><label className="operator-check"><input type="checkbox" checked={frameOnly} onChange={e=>setFrameOnly(e.target.checked)}/> إظهار الفريمات ذات البيانات فقط</label></div>
    <div className="operator-source-grid">{frames.filter(f=>!frameOnly||Number(f.bars)>0).map((f,i)=><article key={txt(f.timeframe,String(i))}><b>{txt(f.timeframe)}</b><span>{f.live===true?'التاريخ حديث':'التاريخ غير حديث أو لم يصل'}</span><small>شموع Core: {count(f.bars)}</small><small>آخر التقاط: {stamp(f.captured_ms)}</small>{txt(f.error,'')&&<small>{txt(f.error)}</small>}</article>)}</div>
    <h4>التجارب والرصد البحثي</h4><div className="operator-measures"><Measure label="نتائج محفوظة" value={count(reports.length)} detail="مقيدة بالحساب في Core"/><Measure label="فريمات ذات ملاحظات" value={count(learning.length)} detail="Wyckoff"/><Measure label="المهمة الجارية" value={txt(data.working,'لا توجد')} detail="الفريم الجاري"/><Measure label="آخر خطأ" value={txt(data.last_error,'لا يوجد خطأ مُبلغ')} detail="لا يعني ضمان خلو النظام من الأخطاء"/></div>
    {learning.length>0&&<div className="operator-source-grid">{learning.map((l,i)=><article key={i}><b>{txt(l.timeframe)} — Wyckoff</b><small>رصد: {count(l.observations)} · مكتمل: {count(l.scored)}</small><small>متوسط صافي افتراضي لكل لوت: {num(l.mean_net_per_lot)===null?'غير متاح':Number(l.mean_net_per_lot).toFixed(2)}</small><span>{l.research_evidence_positive===true?'أدلة بحثية أولية موجبة':'أدلة غير كافية لتقييم إيجابي'}</span></article>)}</div>}
   </>}
   {page==='correlation'&&<><div className="operator-measures"><Measure label="اختبارات تاريخية محفوظة" value={count(data.saved_backtests)} detail="مصادر محتملة للدراسة"/><Measure label="دراسات ارتباط محفوظة" value={count(data.saved_reports)} detail="تقارير في قاعدة Core"/><Measure label="نوع الاستخدام" value={data.research_only===true?'بحث فقط':'غير مؤكد'} detail="لا أوامر تداول"/><Measure label="إعادة النتائج" value={data.deterministic_replay===true?'حتمية':'غير مؤكدة'} detail="عند تطابق المدخلات"/></div>
      <p>الدراسة تحتاج تاريخاً محفوظاً قابلًا للمقارنة. تشغيل اختبارات جديدة يتم من المختبر التاريخي، وليس من هذه الصفحة العامة.</p>
    </>}
   {(page==='analysis'||page==='shadow'||page==='events')&&<><div className="operator-measures"><Measure label="السجلات الراجعة" value={count(entries.length)} detail="آخر دفعة محفوظة من Core"/><Measure label="مطابقة البحث" value={count(filtered.length)} detail="داخل هذه الدفعة فقط"/><Measure label="طبيعة المصدر" value={page==='events'?'سجل التشغيل':page==='shadow'?'قرارات بحثية':'إعدادات فرص بحثية'} detail="لا إثبات تنفيذ بالوسيط"/></div>
    <div className="operator-filters"><label><Search size={16}/> ابحث داخل السجل<input type="search" aria-label={'بحث '+desc.title} value={search} onChange={e=>setSearch(e.target.value)} placeholder="كلمة أو رمز أو نوع الحدث"/></label></div>
    <div className="operator-event-list">{filtered.length?filtered.slice(0,75).map((item,i)=>{
     const record=page==='shadow'?object(item.decision):page==='analysis'?object(item.setup):object(item.detail);
     const label=page==='events'?eventLabel(item.kind):page==='shadow'?txt(object(record.analysis).action,'قرار لم يُسجل'):txt(record.name,txt(record.pattern,'فرصة تاريخية'));
     const when=page==='events'?item.timestamp_ms:page==='analysis'?item.first_seen_ms:item.timestamp_ms;
     const statusText=page==='events'?'حدث محرك':page==='shadow'?'بحث ظل غير منفذ':'فرصة تاريخية';
     return <details key={String(item.id??item.setup_id??i)}><summary><strong>{label}</strong><small>{stamp(when)} · {statusText}</small></summary><EventDetail page={page} item={item}/></details>;
    }):<p className="operator-empty">لا توجد سجلات في رد Core أو لا توجد نتائج تطابق البحث.</p>}</div>
   </>}
   <div className="operator-links">{page==='studio'||page==='correlation'?<button onClick={()=>onNavigate('backtest')}><BookOpen size={16}/> الاختبار التاريخي</button>:null}
    {page==='studio'||page==='correlation'?<button onClick={()=>onNavigate('evaluation')}><BarChart3 size={16}/> تقييم الاستراتيجيات</button>:null}
    {page==='shadow'||page==='analysis'?<button onClick={()=>onNavigate('brainControl')}><Layers3 size={16}/> إدارة العقول</button>:null}
    {page==='events'?<button onClick={()=>onNavigate('execution')}><LineChart size={16}/> فحص التنفيذ</button>:null}
   </div>
  </>}
  <ReadonlyNotice>{desc.notice}</ReadonlyNotice>
 </section>;
}
export function PositionsOperatorWorkspace({positions,currency,accountAvailable,onTrading}:{positions:Position[];currency:string;accountAvailable:boolean;onTrading:()=>void}){
 const [side,setSide]=useState<'all'|'buy'|'sell'>('all');
 const [scope,setScope]=useState<'all'|'owned'>('all');
 const [query,setQuery]=useState('');
 const belongs=(p:Position)=>p.magic!==undefined&&((p.magic>=410600&&p.magic<=410615)||p.magic===410617||p.magic===410618);
 const visible=positions.filter(p=>(side==='all'||(side==='buy'?/buy|شراء/i.test(p.side):/sell|بيع/i.test(p.side)))&&(scope==='all'||belongs(p))&&(!query||[p.symbol,String(p.ticket)].some(x=>x.toLowerCase().includes(query.toLowerCase()))));
 const fmt=(v:unknown)=>num(v)===null?'—':Number(v).toLocaleString('en-US',{maximumFractionDigits:2});
 const gross=positions.reduce((s,p)=>s+(num(p.profit)??0),0);
 return <section className="operator-workspace" dir="rtl" aria-label="مراقبة المراكز المفتوحة">
  <div className="operator-workspace-head"><div><h3><LineChart size={20}/> مراقبة مراكز MT5</h3><p>مراكز الحساب المرصودة، بما فيها صفقات برامج أخرى. الإغلاق التنفيذي متاح من مركز التداول وفق فحوص Core.</p></div><button type="button" onClick={onTrading}>إدارة المراكز في التداول</button></div>
  {!accountAvailable?<p className="operator-evidence" role="status">لا توجد صورة موثقة حالياً لمراكز MT5؛ لا يمكن اعتبار القائمة الخالية تأكيداً لعدم وجود صفقات.</p>:<>
   <div className="operator-measures"><Measure label="المراكز المفتوحة" value={count(positions.length)} detail="جميع رموز MT5"/><Measure label="شراء" value={count(positions.filter(p=>/buy|شراء/i.test(p.side)).length)} detail="وفق نوع المركز"/><Measure label="بيع" value={count(positions.filter(p=>/sell|بيع/i.test(p.side)).length)} detail="وفق نوع المركز"/><Measure label="الربح العائم" value={fmt(gross)+' '+currency} detail="قبل عمولات الإغلاق وأي رسوم إضافية"/></div>
   <div className="operator-filters"><label><Search size={16}/> الرمز أو التذكرة<input type="search" aria-label="بحث المراكز" value={query} onChange={e=>setQuery(e.target.value)}/></label>
    <label>الاتجاه<select aria-label="فلتر اتجاه المراكز" value={side} onChange={e=>setSide(e.target.value as typeof side)}><option value="all">الجميع</option><option value="buy">شراء</option><option value="sell">بيع</option></select></label>
    <label>التبعية<select aria-label="فلتر تبعية المراكز" value={scope} onChange={e=>setScope(e.target.value as typeof scope)}><option value="all">كل المراكز</option><option value="owned">Magic الخاص بالبرنامج فقط</option></select></label></div>
   <div className="operator-table-scroll"><table><thead><tr><th>التذكرة</th><th>الرمز</th><th>الاتجاه</th><th>الحجم</th><th>الدخول</th><th>السعر الحالي</th><th>SL</th><th>TP</th><th>الربح العائم</th><th>Magic</th></tr></thead>
    <tbody>{visible.map(p=><tr key={p.ticket}><td>{p.ticket}</td><td>{p.symbol}</td><td>{p.side}</td><td>{fmt(p.volume)}</td><td>{fmt(p.price_open)}</td><td>{fmt(p.price_current)}</td><td>{p.sl?fmt(p.sl):'—'}</td><td>{p.tp?fmt(p.tp):'—'}</td><td>{fmt(p.profit)} {currency}</td><td>{p.magic??'غير متاح'}</td></tr>)}{!visible.length&&<tr><td colSpan={10} className="operator-empty">لا توجد مراكز تطابق المرشحات في بيانات MT5 الحالية.</td></tr>}</tbody></table></div>
  </>}
  <ReadonlyNotice>STOP لا يغلق المراكز. الربح العائم ليس نتيجة نهائية بعد عمولات الإغلاق والسواب والرسوم.</ReadonlyNotice>
 </section>;
}
