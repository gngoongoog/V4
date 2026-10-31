import {useEffect,useState} from 'react';
import type {Status} from './types';
import {readPanel} from './bridge';

const object=(v:unknown):Record<string,unknown>=>v&&typeof v==='object'&&!Array.isArray(v)?v as Record<string,unknown>:{};
const items=(v:unknown):Record<string,unknown>[]=>Array.isArray(v)?v.filter(x=>x&&typeof x==='object').slice(0,12):[];
const count=(v:unknown)=>typeof v==='number'&&Number.isFinite(v)?v.toLocaleString('en-US'):'—';

/** A read-only diagnostic from already sampled Demo decisions. Never generates signals or broker orders. */
export function SniperDiagnosis({status,preview}:{status:Status|null;preview:boolean}){
 const t=status?.telemetry;
 const identity=t?JSON.stringify([t.server,t.login,t.symbol,t.currency]):'';
 const verified=!preview&&status?.gateway_fresh===true&&t?.connected===true
  &&t.demo===true&&t.account_trade_mode===0;
 const [report,setReport]=useState<Record<string,unknown>|null>(null);
 const [error,setError]=useState('');
 const [loading,setLoading]=useState(false);
 useEffect(()=>{
  let active=true;
  if(!verified){setReport(null);setError('');return ()=>{active=false}};
  const read=async()=>{
   try{
    const r=object(await readPanel('/v1/execution/sniper-diagnosis'));
    if(!active)return;
    setReport(r.context===identity&&r.schema==='sniper-diagnosis-v1'?r:null);
    setError(r.context===identity?'':'رد التشخيص لا يطابق حساب MT5 الحالي');
   }catch(e){if(active){setReport(null);setError(String(e))}}
  };
  void read();
  const timer=setInterval(()=>{void read()},7000);
  return ()=>{active=false;clearInterval(timer)};
 },[identity,verified]);
 const refresh=async()=>{
  if(!verified)return;
  setLoading(true);
  try{
   const r=object(await readPanel('/v1/execution/sniper-diagnosis'));
   if(r.context!==identity||r.schema!=='sniper-diagnosis-v1')throw new Error('هوية حساب MT5 اختلفت');
   setReport(r);setError('');
  }catch(e){setReport(null);setError(String(e))}
  finally{setLoading(false)}
 };
 const obs=object(report?.observations),blockers=object(report?.blockers);
 const causes=items(blockers.top_causes).slice(0,6),recent=items(obs.recent).slice(0,5);
 const mode=String(report?.mode||'—');
 return <section className="operator-card" aria-label="Sniper order blockers">
  <h3>تشخيص القناص — ليش ما يفتح صفقة؟</h3>
  <p>حالة الاتصال وحدها لا تعني سماح Core بالإرسال. المعروض عينات فعلية من قرارات Demo العادية، وليس كل Tick أو إثبات تنفيذ.</p>
  {!verified?<div className="safe-note">التشخيص التفصيلي يحتاج حساب MT5 Demo حديثاً ومتطابقاً. افحص START وربط حساب التنفيذ وصلاحية Algo Trading، وسعر الذهب الحديث.</div>:
   <>
    <div className="quick-ops"><button type="button" disabled={loading} onClick={()=>void refresh()}>تحديث أسباب المنع</button></div>
    {error&&<div className="operator-error" role="alert">{error}</div>}
    {!report?<p>بانتظار قراءة عينات Core للحساب الحالي.</p>:
     <>
      <div className="execution-binding-summary">الوضع المحفوظ: <b>{mode}</b> · المنسق: <b>{String(report.priority_owner||'—')}</b> · START: <b>{report.requested_run===true?'ON':'OFF'}</b> · تصريح التنفيذ: <b>{report.execution_enabled===true?'YES':'NO'}</b></div>
      <div className="execution-binding-summary">عينات القناص: <b>{count(obs.sampled_observations)}</b> · غير جاهزة للدخول: <b>{count(obs.not_entry_ready_observations)}</b> · أوامر مرتبطة ومثبتة في السجل: <b>{count(obs.linked_unique_commands)}</b></div>
      {obs.status==='no_samples'?<p className="safe-note">ماكو عينات بعد لهذا الحساب. خَلِّ النظام يسجل بيانات Demo وتأكد من صلاحية الأسعار والتشغيل قبل الحكم على الاستراتيجية.</p>:null}
      {causes.length>0?<div><strong>الأسباب المتكررة في آخر العينات (قد تتداخل):</strong>
       {causes.map((c,i)=><div className="brain-control-row" key={String(c.code||i)}><b>{String(c.label_ar||c.code||'غير معروف')}</b><span>{count(c.sampled_decisions_with_cause)} عينة</span></div>)}</div>:null}
      {recent.length>0?<div><strong>آخر قرارات القناص المسجلة:</strong>
       {recent.map((r,i)=><div className="brain-control-row" key={String(r.observed_ms)+'-'+i}><span>{r.entry_ready===true?'مؤهل عند الفحص':'لم يدخل'} · {String(r.reason||r.signal_reason||'لا سبب متاح')}</span></div>)}</div>:null}
      <p className="safe-note">هذه القراءات للمتابعة فقط؛ بعض فرص Fast Pulse قد لا تظهر. وجود إشارة مؤهلة لا يعني قبول أمر من الوسيط، وعدم وجود عينات لا يعني أن Core فحص كل الأسعار.</p>
     </>}
   </>}
 </section>;
}
