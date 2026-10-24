import type {Status} from './types';

const obj=(v:unknown):Record<string,unknown>=>
 v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Record<string,unknown>:{};
const count=(v:unknown)=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=0?String(v):'—';
export function eligibleFrozenSniper(status:Status|null,page:unknown):boolean{
 const t=status?.telemetry,config=obj(obj(page).config),trading=obj(config.trading);
 return !!(status?.gateway_fresh&&t?.connected&&t.demo&&t.account_trade_mode===0
  &&Number.isSafeInteger(t.login)&&t.login>0&&t.server&&t.symbol&&t.currency
  &&trading.mode==='sniper'&&trading.sniper_brain==='learner');
}
export function SniperFrozenOperations({status,page,report,preview,disabled,onRefresh,onAction}:{
 status:Status|null;page:unknown;report:unknown;preview:boolean;disabled:boolean;
 onRefresh:()=>void;onAction:(action:'freeze'|'stop')=>Promise<void>;
}){
 const t=status?.telemetry,view=obj(report);
 const eligible=!preview&&eligibleFrozenSniper(status,page);
 const expected=t?JSON.stringify([t.server,t.login,t.symbol,t.currency]):'';
 const statusValue=view.status;
 const scoped=eligible&&view.context===expected&&view.schema==='p1k3-sniper-frozen-tick-v1'
    &&view.mode==='sniper'&&view.sniper_brain==='learner'
    &&view.no_broker_orders===true&&view.independent_oos===false
    &&view.paper_pnl===null&&view.broker_pnl===null
    &&(statusValue==='active'||statusValue==='stopped');
 const active=scoped&&statusValue==='active';
 const stopped=status?.requested_run===false;
 const ready=eligible&&stopped&&!disabled;
 const canFreeze=ready&&(!scoped||!active);
 const canStop=ready&&active;
 return <section className="operator-card" aria-label="Frozen Sniper Learner forward Tick decisions">
  <h3>Sniper Learner — مقارنة Tick المستقبلية P1-K3</h3>
  <p>تجميد نسخة من نموذج Learner على Demo، ثم مقارنة إشاراتها الخام مع النموذج
    المتكيّف على نفس أسعار Tick المقبولة بعد التجميد. لا نتائج أرباح أو معاملات MT5 إضافية.</p>
  {!scoped?<p role="status" className="safe-note">
   {eligible?'لا توجد تجربة مجمّدة مؤكدة لهذا الحساب؛ هذا ليس تقرير ربحية.':
    'يتطلب حساب Demo حديثاً ووضع Sniper مع العقل Learner. Classic خارج هذه الدراسة.'}
  </p>:<div className="operator-grid">
   <div>حالة التجربة <strong>{active?'نشطة':'متوقفة'}</strong></div>
   <div>تيكات مستقبلية مقبولة <strong>{count(view.accepted_prospective_pulses)}</strong></div>
   <div>قرارات قابلة للمقارنة <strong>{count(view.comparable_future_tick_decisions)}</strong></div>
   <div>قرارات متطابقة <strong>{count(view.exact_decisions)}</strong></div>
   <div>اختلاف جاهزية <strong>{count(view.readiness_disagreements)}</strong></div>
   <div>اختلاف اتجاه <strong>{count(view.side_disagreements)}</strong></div>
   <div>اختلاف هدف/وقف <strong>{count(view.target_or_stop_disagreements)}</strong></div>
   <div>فجوات مصادر مستبعدة <strong>{count(view.excluded_source_switch)}</strong></div>
   <div>فواصل سعرية مستبعدة <strong>{count(view.excluded_broker_clock_gap)}</strong></div>
   <div>فقد طابور Native مستبعد <strong>{count(view.excluded_native_queue_counter_change)}</strong></div>
   <div>لقطات MT5 خارج Pulse (استبعاد) <strong>{count(view.unpaired_reconciled_quote_changes)}</strong></div>
   <div>تغيّر سعر بنفس ملي ثانية الوسيط (مستبعد) <strong>{count(view.unpaired_same_clock_price_changes)}</strong></div>
  </div>}
  <div className="quick-ops">
   <button type="button" disabled={preview||disabled} onClick={onRefresh}>تحديث من Rust Core</button>
   <button type="button" disabled={!canFreeze} onClick={()=>void onAction('freeze')}>تجميد نموذج Sniper Learner</button>
   <button type="button" disabled={!canStop} onClick={()=>void onAction('stop')}>إيقاف المقارنة البحثية</button>
  </div>
  <p className="safe-note">تحتاج STOP لإنشاء/إيقاف التجربة؛ النموذج المجمد يحتفظ بنتائجه
   في ذاكرة Core فقط ويبدأ برصد تيكات مستقبلية مع فترة تهيئة. التوافق ليس معدل نجاح،
   والعينة ليست اختبار OOS مستقلاً أو تصريحاً بالتداول الحقيقي.</p>
 </section>;
}
