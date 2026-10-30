import {useEffect,useRef,useState} from 'react';
import {ShieldCheck,TrendingUp} from 'lucide-react';
import type {Status} from './types';
import type {PositionProtectionPolicy} from './bridge';

const object=(value:unknown):Record<string,unknown>=>value&&typeof value==='object'&&!Array.isArray(value)?value as Record<string,unknown>:{};
type Draft={break_even_enabled:boolean;break_even_trigger_points:string;break_even_offset_points:string;trailing_enabled:boolean;trailing_distance_points:string;trailing_step_points:string};
const defaults:Draft={break_even_enabled:false,break_even_trigger_points:'10',break_even_offset_points:'3',trailing_enabled:false,trailing_distance_points:'15',trailing_step_points:'5'};
const stateLabel:Record<string,string>={queued:'بالطابور',dispatched:'قيد الإرسال',placed:'بانتظار ظهور الوقف في MT5',unknown:'النتيجة غير مؤكدة',filled:'بانتظار تحقق Core',observed:'الوقف ظاهر في MT5',rejected:'رفض الوسيط أو فشل التحقق',cancelled:'أُلغي قبل الإرسال',expired:'انتهت الصلاحية',position_absent:'المركز لم يعد ظاهراً؛ النتيجة محفوظة للمراجعة'};

export function PositionProtectionOperations({status,source,disabled,onSet}:{
 status:Status|null;source:unknown;disabled:boolean;onSet:(policy:PositionProtectionPolicy,expectedPolicyId:string)=>Promise<boolean>;
}){
 const [draft,setDraft]=useState<Draft>(defaults),[dirty,setDirty]=useState(false),basePolicy=useRef('');
 const t=status?.telemetry,view=object(source),protection=object(view.position_protection),policy=object(protection.policy),settings=object(view.settings);
 const context=t?JSON.stringify([t.server,t.login,t.symbol,t.currency]):'';
 const available=!!t&&status?.gateway_fresh===true&&t.connected&&view.account_matches===true&&view.context===context&&
  settings.login===t.login&&settings.server===t.server&&settings.currency===t.currency&&
  settings.target===(t.demo?'demo':'live')&&t.account_trade_mode===(t.demo?0:2)&&
  protection.scope==='owned_positions_bound_account_symbol_and_mode'&&typeof protection.policy_id==='string'&&typeof policy.break_even_enabled==='boolean'&&typeof policy.trailing_enabled==='boolean';
 const policyText=JSON.stringify(policy);
 useEffect(()=>{
  if(dirty||!available)return;
  basePolicy.current=String(protection.policy_id);
  setDraft({break_even_enabled:policy.break_even_enabled===true,break_even_trigger_points:String(policy.break_even_trigger_points),
   break_even_offset_points:String(policy.break_even_offset_points),trailing_enabled:policy.trailing_enabled===true,
   trailing_distance_points:String(policy.trailing_distance_points),trailing_step_points:String(policy.trailing_step_points)});
 },[policyText,available,dirty]);
 const values={break_even_enabled:draft.break_even_enabled,break_even_trigger_points:Number(draft.break_even_trigger_points),
  break_even_offset_points:Number(draft.break_even_offset_points),trailing_enabled:draft.trailing_enabled,
  trailing_distance_points:Number(draft.trailing_distance_points),trailing_step_points:Number(draft.trailing_step_points)};
 const nums=[draft.break_even_trigger_points,draft.break_even_offset_points,draft.trailing_distance_points,draft.trailing_step_points];
 const valid=nums.every(v=>v.trim()!==''&&Number.isInteger(Number(v))&&Number(v)>=0&&Number(v)<=1000000)&&
  values.break_even_trigger_points>=1&&values.break_even_offset_points<=values.break_even_trigger_points&&
  values.trailing_distance_points>=1&&values.trailing_step_points>=1;
 const stale=dirty&&basePolicy.current!==protection.policy_id;
 const locked=disabled||!available||stale;
 const edit=(value:Partial<Draft>)=>{setDraft(old=>({...old,...value}));setDirty(true)};
 const commands=Array.isArray(protection.commands)?protection.commands.slice(0,4):[];
 return <section className="position-protection" aria-label="حماية المراكز بالتعادل والوقف المتحرك">
  <div className="protection-head"><ShieldCheck size={19}/><strong>حماية المراكز المفتوحة</strong><small>{available?'إعدادات الحساب والرمز الحالي':'بانتظار حساب MT5 مرتبط وحديث'}</small></div>
  <form onSubmit={e=>{e.preventDefault();if(locked||!valid||!dirty)return;void onSet(values,basePolicy.current).then(ok=>{if(ok)setDirty(false)})}}>
   <div className="protection-cards"><fieldset><legend>نقطة التعادل</legend>
    <label className="protection-toggle"><input type="checkbox" checked={draft.break_even_enabled} disabled={locked} onChange={e=>edit({break_even_enabled:e.target.checked})}/> تفعيل نقل الوقف إلى التعادل</label>
    <label>الربح لبدء التعادل (نقطة)<input aria-label="الربح لبدء التعادل" type="number" min="1" max="1000000" step="1" value={draft.break_even_trigger_points} disabled={locked} onChange={e=>edit({break_even_trigger_points:e.target.value})}/></label>
    <label>هامش بعد الدخول (نقطة)<input aria-label="هامش التعادل" type="number" min="0" max="1000000" step="1" value={draft.break_even_offset_points} disabled={locked} onChange={e=>edit({break_even_offset_points:e.target.value})}/></label>
   </fieldset><fieldset><legend><TrendingUp size={15}/> الوقف المتحرك</legend>
    <label className="protection-toggle"><input type="checkbox" checked={draft.trailing_enabled} disabled={locked} onChange={e=>edit({trailing_enabled:e.target.checked})}/> تفعيل الوقف المتحرك</label>
    <label>المسافة عن السعر (نقطة)<input aria-label="مسافة الوقف المتحرك" type="number" min="1" max="1000000" step="1" value={draft.trailing_distance_points} disabled={locked} onChange={e=>edit({trailing_distance_points:e.target.value})}/></label>
    <label>أقل خطوة للتحديث (نقطة)<input aria-label="خطوة الوقف المتحرك" type="number" min="1" max="1000000" step="1" value={draft.trailing_step_points} disabled={locked} onChange={e=>edit({trailing_step_points:e.target.value})}/></label>
   </fieldset></div>
   {!valid&&<small role="alert">أدخل نقاطاً صحيحة؛ هامش التعادل لا يتجاوز ربح البدء.</small>}
   {stale&&<small role="alert">تغيّرت الإعدادات المحفوظة أثناء التعديل. <button type="button" onClick={()=>setDirty(false)}>استرجاع الإعدادات الحالية</button></small>}
   <button className="save-button" type="submit" disabled={locked||!valid||!dirty}>حفظ إعدادات حماية المراكز</button>
  </form>
  <p>تعمل على مراكز BrainTrade المملوكة للرمز الحالي فقط، حتى بعد STOP، عند اتصال MT5. الوقف المتحرك يبدأ عندما يتحرك السعر بمقدار المسافة المطلوبة. النقطة حسب وسيطك{typeof t?.point==='number'?` (${t.point})`:''}. الهدف TP يبقى كما هو؛ قد يرفض الوسيط التعديل.</p>
  <p>التعادل محسوب من سعر الدخول قبل العمولة والسواب والانزلاق؛ ليس ضماناً لربح صافٍ. إذا لم يطابق سعر التعادل خطوة السعر لدى الوسيط فلن يُرسل. انقطاع البرنامج أو MT5 يوقف التحديثات الجديدة، والوقف الموجود يبقى لدى الوسيط.</p>
  {commands.length>0&&<div className="protection-results"><strong>آخر تعديلات الوقف</strong>{commands.map((item,i)=>{const command=object(item),request=object(command.request),state=String(command.state||'unknown');return <span key={String(command.id||i)}>#{String(request.position||'—')} · {stateLabel[state]||state}</span>})}</div>}
 </section>;
}
