import {useEffect,useState} from 'react';
import type {Status} from './types';
import {readPanel} from './bridge';
import {learningAccountView} from './learning-account-evidence';

type Slot={scope:string;report:unknown};
export function LearningAccountOperations({status,scope,preview}:{
 status:Status|null;scope:string;preview:boolean;
}){
 const [slot,setSlot]=useState<Slot|null>(null);
 const [error,setError]=useState('');
 const [revision,setRevision]=useState(0);
 useEffect(()=>{
  if(preview){setSlot(null);setError('');return}
  let alive=true,working=false;
  const run=async()=>{
   if(!alive||working||document.visibilityState!=='visible')return;
   working=true;
   try{
    const report=await readPanel('/v1/learning/account-evidence');
    if(alive){setSlot({scope,report});setError('')}
   }catch(e){if(alive){setSlot(null);setError(String(e))}}
   finally{working=false}
  };
  setSlot(null);setError('');
  void run();
  const interval=setInterval(()=>void run(),12000);
  const visible=()=>{if(document.visibilityState==='visible')void run()};
  document.addEventListener('visibilitychange',visible);
  return()=>{alive=false;clearInterval(interval);document.removeEventListener('visibilitychange',visible)};
 },[scope,preview,revision]);
 const v=slot?.scope===scope?learningAccountView(status,slot.report,Date.now()):null;
 return <section className="operator-card" aria-label="Verified account research learning evidence">
  <h3>تقدّم بيانات التعلّم — حساب MT5 الحالي</h3>
  <p>بيانات شموع M1 من Rust Core بعد التحقق من الحساب والخادم والرمز وDemo/REAL. هذه عينات بحثية، وليست صفقات مغلقة أو نسبة نجاح استراتيجية.</p>
  <div className="quick-ops"><button type="button" disabled={preview} onClick={()=>setRevision(x=>x+1)}>تحديث بيانات التعلّم</button></div>
  {preview?<p className="safe-note">DESIGN PREVIEW — لا توجد عينة تعلم مرتبطة بالوسيط.</p>:
   !slot?<p className="safe-note">{error?'تعذرت قراءة Core: '+error:'جار قراءة سجل الشموع المطابق للحساب...'}</p>:
   !v?.ready?<p className="safe-note">{v?.reason||'الحساب غير موثق'}</p>:
   <>
    <p className="safe-note">{v.mode} · {v.reason}</p>
    <div className="operator-grid">
     <div>شموع M1 المحفوظة<strong>{v.snapshots}</strong></div>
     <div>عينات ناضجة متوافقة مع مخطط التدريب<strong>{v.eligible}</strong></div>
     <div>عينات بانتظار النضج<strong>{v.pending}</strong></div>
     <div>عينات مستبعدة لتعارض المخطط<strong>{v.incompatible}</strong></div>
     <div>آخر توقيت شمعة Unix<strong>{v.lastBar??'—'}</strong></div>
     <div>تدريب النموذج التلقائي<strong>غير مفعل في هذا السجل</strong></div>
    </div>
    <p className="safe-note">هذه العدادات ليست عدد صفقات MT5 ولا دليلاً على ربحية المستقبل. النماذج البحثية والسجلات العامة بالأسفل قد تتضمن حسابات أو مراحل سابقة؛ لا نخلطها بإحصائيات الحساب الحالي. لا توجد ترقية تلقائية للعقل.</p>
   </>}
 </section>;
}
