import {useEffect,useState} from 'react';
import {readPanel} from './bridge';

type Group={scope:string;kind:string;state:string;count:number;last_age_seconds:number};
type Audit={
 schema:string;read_only:boolean;blocked_settings:boolean;pending_total:number;
 current_session_pending:number;other_or_unverified_session_pending:number;
 current_account_verified:boolean;oldest_age_seconds:number;
 recent_groups:Group[];
};
const count=(n:number|undefined)=>typeof n==='number'&&Number.isFinite(n)?n.toLocaleString('en-US'):'—';

/** Read-only, secret-free explanation of Core's global broker intent save gate. */
export function ReconciliationDiagnostics({preview}:{preview:boolean}){
 const [audit,setAudit]=useState<Audit|null>(null);
 const [error,setError]=useState('');
 const [loading,setLoading]=useState(false);
 const refresh=async()=>{
  if(preview)return;
  setLoading(true);
  try{
   const received=await readPanel('/v1/execution/reconciliation') as Audit;
   if(received?.schema!=='broker-reconciliation-v1'||received.read_only!==true)
    throw Error('Invalid Core reconciliation diagnostic response');
   setAudit(received);setError('');
  }catch(e){setAudit(null);setError(String(e))}
  finally{setLoading(false)}
 };
 useEffect(()=>{
  if(preview){setAudit(null);return}
  let alive=true;
  const fetchReport=async()=>{
   try{
    const response=await readPanel('/v1/execution/reconciliation') as Audit;
    if(!alive)return;
    if(response?.schema==='broker-reconciliation-v1'&&response.read_only===true){
     setAudit(response);setError('');
    } else {setAudit(null);setError('Core diagnostic unavailable')}
   }catch(e){if(alive){setAudit(null);setError(String(e))}}
  };
  void fetchReport();
  const timer=setInterval(()=>{void fetchReport()},10_000);
  return ()=>{alive=false;clearInterval(timer)};
 },[preview]);
 const groups=Array.isArray(audit?.recent_groups)?audit.recent_groups.slice(0,10):[];
 return <section className="operator-card" aria-label="Broker reconciliation diagnostics">
  <h3>حالة تسوية أوامر الوسيط — سبب منع الحفظ</h3>
  <p>الفحص يقرأ سجل Core فقط؛ ما يحذف أو يغيّر أو يعيد إرسال أي أمر.</p>
  <div className="quick-ops"><button type="button" disabled={preview||loading}
   onClick={()=>void refresh()}>تحديث حالة المصالحة</button></div>
  {error?<div className="operator-error" role="alert">{error}</div>:null}
  {!audit?<p className="safe-note">بيانات المصالحة غير متاحة حالياً.</p>:
   <><div className="execution-binding-summary">
     الأوامر غير المحسومة: <b>{count(audit.pending_total)}</b> · الجلسة الحالية:
     <b>{count(audit.current_session_pending)}</b> · جلسات أخرى/غير موثقة:
     <b>{count(audit.other_or_unverified_session_pending)}</b>
    </div>
    {audit.blocked_settings?
     <p className="operator-error">Core يمنع حفظ تغييرات التنفيذ لأن الأوامر غير محسومة. STOP لا يعني أن الوسيط أكد نتيجتها؛ لا تحذف قاعدة البيانات أو تعدل حالة الأوامر يدوياً.</p>:
     <p className="safe-note">ماكو أمر غير محسوم في سجل Core حالياً. إذا الحفظ ما زال مرفوضاً، حدّث الصفحة وتأكد من STOP وصحة الاتصال.</p>}
    {!audit.current_account_verified?<p className="safe-note">الحساب الحالي غير موثّق باتصال MT5 حديث؛ لا يمكن إسناد الأوامر لجلسة حالية بدقة.</p>:null}
    {groups.length>0?<div>
     <strong>تفاصيل مختصرة بدون أرقام الحسابات:</strong>
     {groups.map((g,i)=><div className="brain-control-row" key={i}>
      <span>{g.scope==='current'?'الحساب الحالي':g.scope==='other'?'جلسة أخرى':'جلسة غير مؤكدة'} · {g.kind} · {g.state}</span>
      <b>{count(g.count)} · آخر أمر قبل {count(g.last_age_seconds)} ث</b>
     </div>)}
    </div>:null}
   </>}
  <p className="safe-note">تسوية أمر `unknown` أو `partial` تحتاج إثباتاً من سجل MT5، وليس زر تجاهل أو تصفير. أثناء بقاء أمر جزئي بلا نتيجة مؤكدة قد يتأخر الإغلاق الزمني؛ راقب المركز وأوامر الوسيط وتأكد من وقف الخسارة في MT5.</p>
 </section>;
}
