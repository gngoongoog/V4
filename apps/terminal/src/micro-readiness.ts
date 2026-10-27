// P1-E read-only attestation of Core's permanent Micro REAL recovery latch.
// This is NOT an order-admission API; Core, Gateway and MT5 recheck separately.
import type {MicroStatus} from './micro-capital';

type Obj=Record<string,unknown>;
const obj=(x:unknown):Obj=>x!==null&&typeof x==='object'&&!Array.isArray(x)?x as Obj:{};
const positive=(x:unknown):x is number=>typeof x==='number'&&Number.isFinite(x)&&x>0;
const number=(x:unknown):x is number=>typeof x==='number'&&Number.isFinite(x);
export type MicroAttestation={
 verified:boolean;state:string;label:string;reason:string;candidate:boolean;
 capPercent:number|null;recoveryEquity:number|null;observedMs:number|null;
 dailyState:string;dailyEnabled:boolean|null;dailyReason:string;
};
const missing:MicroAttestation={
 verified:false,state:'unverified',label:'غير مؤكد',reason:'لا تتوفر قراءة Core حديثة ومطابقة لحساب الوسيط',
 candidate:false,capPercent:null,recoveryEquity:null,observedMs:null,
 dailyState:'unverified',dailyEnabled:null,dailyReason:'حالة القاطع اليومي غير موثقة'
};
export function microAttestation(status:MicroStatus|null,monitor:unknown,nowMs:number):MicroAttestation {
 const t=status?.telemetry;
 const m=obj(monitor),a=obj(m.account),micro=obj(m.micro_real),b=obj(micro.account);
 const observed=m.observed_ms;
 if(!status||status.gateway_fresh!==true||!t||t.connected!==true||
   !Number.isSafeInteger(t.login)||!positive(t.login)||!t.server||!t.currency||
   !((t.demo===true&&t.account_trade_mode===0)||(t.demo===false&&t.account_trade_mode===2))||
   !Number.isSafeInteger(nowMs)||!number(observed)||!Number.isSafeInteger(observed)||
   observed<=0||observed>nowMs+2000||nowMs-observed>15000||
   m.micro_real===undefined||
   micro.schema!=='p1e-micro-recovery-v1'||micro.read_only!==true||
   micro.broker_execution_authorized!==false||micro.observed_ms!==observed||
   a.login!==t.login||a.server!==t.server||a.currency!==t.currency||a.mode!==t.account_trade_mode||
   b.login!==t.login||b.server!==t.server||b.currency!==t.currency||b.mode!==t.account_trade_mode||
   typeof micro.state!=='string'||typeof micro.candidate_only!=='boolean'||
   typeof micro.configured!=='boolean'||typeof micro.recovered_persisted!=='boolean'||
   typeof micro.current_milestone_reached!=='boolean'||
   !positive(micro.cap_percent)||micro.cap_percent>20||
   !positive(micro.recovery_equity)||micro.recovery_equity<50||
   typeof m.state!=='string'||typeof m.enabled!=='boolean'
 )return {...missing};
 const state=micro.state;
 const real=t.account_trade_mode===2&&!t.demo;
 const statuses=['candidate_only','recovered_latched','milestone_reached','disabled','unavailable','unverified_or_demo'];
 if(!statuses.includes(state)||micro.candidate_only!==(state==='candidate_only')||
    (state==='candidate_only'&&(!real||micro.recovered_persisted||micro.current_milestone_reached||!micro.configured))||
    (state==='recovered_latched'&&!micro.recovered_persisted)||
    (state==='milestone_reached'&&!micro.current_milestone_reached)||
    (state==='disabled'&&micro.configured)||
    (state==='unverified_or_demo'&&real)||
    (real&&(state==='unverified_or_demo'))||
    (m.state==='locked'&&m.locked!==true)){
   return {...missing};
 }
 let label='غير مؤهل';let reason='Micro REAL غير متاح للحساب الحالي';
 if(!real){label='DEMO';reason='وضع Micro REAL يعمل فقط على حساب حقيقي مطابق'}
 else if(state==='candidate_only'){label='مرشح · يحتاج تحقق التنفيذ';reason='لم يصل Equity إلى حد التعافي ولم يوجد قفل دائم، لكن هذا ليس تصريحاً بفتح صفقة'}
 else if(state==='recovered_latched'){label='انتهى نهائياً';reason='Core سجل الوصول إلى Equity التعافي لهذا الحساب؛ نزول الرصيد بعد ذلك لا يعيد Micro'}
 else if(state==='milestone_reached'){label='بلغ حد التعافي';reason='Equity بلغ حد التعافي الحالي؛ لن يطبق سقف Micro للصفقات الجديدة'}
 else if(state==='disabled'){label='مطفأ باختيارك';reason='الإعداد Micro REAL غير مفعّل لدى Core'}
 const dailyState=m.state;
 let dailyReason=typeof m.reason==='string'?m.reason:'حالة القاطع اليومي غير واضحة';
 if(m.enabled===false)dailyReason+=' — القاطع اليومي مطفأ بإعداد المستخدم';
 return {
  verified:true,state,label,reason,candidate:real&&state==='candidate_only',
  capPercent:micro.cap_percent,recoveryEquity:micro.recovery_equity,
  observedMs:observed,dailyState,dailyEnabled:m.enabled,dailyReason
 };
}
