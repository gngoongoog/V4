import type {Status} from './types';
import {microAttestation} from './micro-readiness';

export type DailyRiskView={
 verified:boolean;state:string;message:string;mode:'DEMO'|'REAL'|null;
 enabled:boolean|null;persistedLock:boolean|null;day:string|null;
 dayLoss:number|null;allowedLoss:number|null;remaining:number|null;
 lossPercent:number|null;limitPercent:number|null;baseline:number|null;
 accountCurrency:string|null;warning:string;
};
const unknown=(message:string):DailyRiskView=>({
 verified:false,state:'unavailable',message,mode:null,enabled:null,
 persistedLock:null,day:null,dayLoss:null,allowedLoss:null,remaining:null,
 lossPercent:null,limitPercent:null,baseline:null,accountCurrency:null,
 warning:'لا تعتبر غياب التقرير تصريحاً بالتداول؛ Core وMT5 هما مصدر قرار التنفيذ.'
});
const obj=(x:unknown):Record<string,unknown>=>x!==null&&typeof x==='object'&&!Array.isArray(x)?x as Record<string,unknown>:{};
const num=(v:unknown):v is number=>typeof v==='number'&&Number.isFinite(v);
export function dailyRiskView(status:Status|null,source:unknown,now:number):DailyRiskView{
 const m=obj(source),t=status?.telemetry;
 // Existing Micro evidence independently checks both Core account identities,
 // mode agreement, generation freshness, and persisted recovery evidence.
 const att=microAttestation(status,source,now);
 if(!att.verified||!t||!Number.isSafeInteger(now))
  return unknown('لا تتوفر شهادة Core حديثة تطابق حساب MT5 الحالي ونوع Demo/REAL');
 const state=m.state;
 if(!['ready','locked','breached','disabled','unavailable'].includes(String(state))||
    typeof m.enabled!=='boolean'||typeof m.locked!=='boolean'||
    typeof m.day!=='string'||!/^[0-9]{4}-[0-9]{2}-[0-9]{2}$/.test(m.day)||
    !num(m.limit_percent)||m.limit_percent<=0||m.limit_percent>100||
    (state==='ready'&&(m.enabled!==true||m.locked!==false||m.data_valid!==true))||
    (state==='locked'&&(m.enabled!==true||m.locked!==true))||
    (state==='breached'&&(m.enabled!==true||m.data_valid!==true))||
    (state==='disabled'&&m.enabled!==false))
  return unknown('Core أرسل بيانات قاطع خسارة متعارضة أو غير مكتملة');
 const trustworthy=(m.data_valid===true &&
    num(m.estimated_day_start_balance)&&m.estimated_day_start_balance>0&&
    num(m.estimated_day_loss)&&num(m.allowed_day_loss)&&m.allowed_day_loss>=0&&
    num(m.estimated_day_loss_percent)&&
    Math.abs(m.allowed_day_loss-m.estimated_day_start_balance*m.limit_percent/100)
       <=Math.max(0.01,Math.abs(m.allowed_day_loss)*1e-6));
 // READY or BREACHED are comparisons against the actual configured budget.
 // A mathematically inconsistent allowance must invalidate these claims;
 // only a durable LOCKED flag survives missing/inconsistent estimate math.
 if((state==='ready'||state==='breached')&&!trustworthy)
  return unknown('تقرير Core فيه تعارض بين حد الخسارة والرصيد ونسبة المخاطرة');
 const known=(x:unknown):number|null=>trustworthy&&num(x)?x:null;
 const allowed=known(m.allowed_day_loss),loss=known(m.estimated_day_loss);
 // If daily breaker is disabled there is no enforced daily allowance.
 const remaining=m.enabled===true&&allowed!==null&&loss!==null?
    Math.max(0,allowed-loss):null;
 const message=state==='locked'?'قاطع الخسارة مسجل ومقفل — لا تدخل صفقات جديدة':
  state==='breached'?'خسارة اليوم تجاوزت الحد المرصود — يلزم فحص Core النهائي':
  state==='disabled'?'قاطع الخسارة اليومي مطفأ بإعداد المستخدم':
  state==='ready'?'قاطع اليوم مفعّل، ولم تظهر تجاوزات مؤكدة في آخر تحديث':
  'قراءة قاطع الخسارة غير متاحة';
 return {verified:true,state:String(state),message,mode:t.demo?'DEMO':'REAL',
  enabled:m.enabled,persistedLock:m.locked,day:m.day,
  dayLoss:loss,allowedLoss:allowed,remaining,
  lossPercent:known(m.estimated_day_loss_percent),limitPercent:m.limit_percent,
  baseline:known(m.estimated_day_start_balance),
  accountCurrency:t.currency,
  warning:state==='disabled'&&m.locked?
   'القاطع مطفأ، لكن سجل قفل سابق ما زال محفوظاً في Core. لا تمسح الحالة من هذه الواجهة.':
   'الأرقام تقديرية من Equity ونتيجة اليوم؛ الإيداعات والسحب والمراكز العابرة لمنتصف الليل قد تغيّر الحساب. هذا التقرير للعرض فقط، وليس إذن دخول.'
 };
}
