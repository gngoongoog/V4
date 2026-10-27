import type {Status} from './types';

const record=(v:unknown):Record<string,unknown>=>v&&typeof v==='object'
 &&!Array.isArray(v)?v as Record<string,unknown>:{};
const validCount=(v:unknown)=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=0;
const renderCount=(v:unknown)=>validCount(v)?String(v):'—';
export function sniperTickEvidence(status:Status|null,report:unknown,preview:boolean,now:number){
 const empty=(reason:string)=>({available:false,reason,data:{} as Record<string,unknown>});
 if(preview)return empty('معاينة التصميم: لا توجد أدلة Tick حقيقية');
 const t=status?.telemetry;
 if(!status?.gateway_fresh||!t?.connected||!t.demo||t.account_trade_mode!==0||
   !Number.isSafeInteger(t.login)||t.login<=0)
  return empty('يتطلب حساب MT5 Demo موصولاً ومتطابقاً');
 const v=record(report);
 const a=record(v.account);
 if(v.schema!=='p1k2-sniper-accepted-pulse-audit-v1'||v.status!=='observed'||
  v.read_only!==true||v.no_broker_orders!==true||v.broker_tick_delivery_complete!==false||
  v.estimated_missing_market_ticks!==null||v.observed_market_tick_count!==null||
  v.context!==JSON.stringify([t.server,t.login,t.symbol,t.currency])||
  a.login!==t.login||a.server!==t.server||a.symbol!==t.symbol||a.currency!==t.currency)
  return empty('لا توجد تيكات قناص مقبولة موثّقة لنفس جلسة Demo');
 const at=typeof v.last_accepted_ms==='number'?v.last_accepted_ms:NaN;
 if(!validCount(at)||at>now+2000||now-at>15_000)
  return empty('رصد القناص متوقف أو متقادم، ولا يجوز عرضه كتيكات مباشرة');
 for(const name of ['accepted_pulses','accepted_native_pulses',
  'accepted_poll_fallback_pulses','accepted_unknown_origin_pulses',
  'native_poll_source_switches','observed_quote_gaps_gt_1500ms',
  'largest_observed_quote_gap_ms','same_broker_millisecond_pulses']){
   if(!validCount(v[name]))return empty('بيانات عدّادات تيكات غير موثّقة');
 }
 return {available:true,reason:'accepted_pulses_only',data:v};
}
export function SniperTickEvidence({status,report,preview,onRefresh}:{
 status:Status|null;report:unknown;preview:boolean;onRefresh:()=>void;
}){
 const view=sniperTickEvidence(status,report,preview,Date.now());
 const v=view.data;
 return <section className="operator-card" aria-label="Sniper Native and Poll accepted-pulse provenance">
  <h3>مراقبة تيكات القناص — P1-K2</h3>
  <p>هذه الأعداد تمثل الرسائل التي قبلها Rust Core فعلياً، مو جميع تيكات السوق.
    الثغرات الزمنية تعني عدم وجود اقتباس مقبول خلال الفترة، وليس عدد تيكات مفقودة.</p>
  {!view.available?<p role="status" className="safe-note">{view.reason}</p>:
   <div className="operator-grid">
    <div>تيكات مقبولة <strong>{renderCount(v.accepted_pulses)}</strong></div>
    <div>من Native EA <strong>{renderCount(v.accepted_native_pulses)}</strong></div>
    <div>من Poll <strong>{renderCount(v.accepted_poll_fallback_pulses)}</strong></div>
    <div>مصدر غير موثّق <strong>{renderCount(v.accepted_unknown_origin_pulses)}</strong></div>
    <div>تبديل المصدر <strong>{renderCount(v.native_poll_source_switches)}</strong></div>
    <div>فجوات مرصودة أكثر من 1.5 ثانية <strong>{renderCount(v.observed_quote_gaps_gt_1500ms)}</strong></div>
    <div>أطول فجوة مرصودة ms <strong>{renderCount(v.largest_observed_quote_gap_ms)}</strong></div>
    <div>تغيّرات بنفس ملي ثانية الوسيط <strong>{renderCount(v.same_broker_millisecond_pulses)}</strong></div>
    <div>تجاوز طابور Native بعد خط الأساس <strong>{renderCount(v.reported_native_mailbox_overflow_since_baseline)}</strong></div>
   </div>}
  <p className="safe-note">الرصد في الذاكرة فقط ويُصفّر عند إعادة تشغيل Core أو تبديل الحساب.
   عداد تجاوز Native ليس ضماناً لاكتمال الدفق، ولا يوجد بعد تقييم استراتيجية Sniper مجمدة أو أرباح Paper معتمدة.</p>
  <button type="button" disabled={preview} onClick={onRefresh}>تحديث تقرير Tick من Core</button>
 </section>;
}
