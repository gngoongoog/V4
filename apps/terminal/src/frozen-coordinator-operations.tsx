import type {Status} from './types';

export type SupportedFrozenMode='expert'|'scalping'|'swing';
const MODE_NAMES:Record<SupportedFrozenMode,string>={
 expert:'Expert',scalping:'Scalping',swing:'Swing'
};
const record=(v:unknown):Record<string,unknown>=>
 v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Record<string,unknown>:{};
const counter=(v:unknown):string=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=0?String(v):'—';
const amount=(v:unknown):string=>typeof v==='number'&&Number.isFinite(v)
 ?v.toLocaleString('en-US',{maximumFractionDigits:2}):'—';
export function supportedFrozenMode(mode:unknown):mode is SupportedFrozenMode{
 return mode==='expert'||mode==='scalping'||mode==='swing';
}
export function frozenModeEligible(status:Status|null,mode:unknown,context:unknown):boolean{
 const t=status?.telemetry;
 return !!(status?.gateway_fresh&&t?.connected&&t.demo&&t.account_trade_mode===0
    &&t.login>0&&t.server&&t.symbol&&t.currency&&supportedFrozenMode(mode)
    &&context===JSON.stringify([t.server,t.login,t.symbol,t.currency]));
}
export function FrozenCoordinatorOperations({status,source,currentMode,brainContext,armed,
 disabled,onRefresh,onAction}:{
 status:Status|null;source:unknown;currentMode:unknown;brainContext:unknown;
 armed:boolean;disabled:boolean;onRefresh:()=>void;
 onAction:(action:'freeze'|'stop')=>Promise<void>;
}){
 const eligible=frozenModeEligible(status,currentMode,brainContext);
 const data=record(source);
 const state=typeof data.status==='string'?data.status:'unavailable';
 const ready=eligible&&data.mode===currentMode&&data.context===brainContext
    &&data.no_broker_orders===true&&['not_started','active','stopped'].includes(state);
 const active=ready&&state==='active';
 const canMutate=ready&&!armed&&!disabled;
 const expert=currentMode==='expert';
 const paper=expert?record(data.paired_quote_paper):{};
 const title=supportedFrozenMode(currentMode)?MODE_NAMES[currentMode]:'Sniper';
 return <section className="operator-card" aria-label="Frozen selected-mode coordinator forward decisions">
  <h3>السياسة المجمدة لمنسق {title} — P1-J</h3>
  <p>بحث Demo فقط: مقارنة قرار النظام المتكيّف مقابل أوزان وإعدادات مجمدة مسبقاً
     على شموع M1 المكتملة بعد تاريخ التجميد. لا يرسل أوامر MT5 ولا يدرّب العقول.</p>
  {!ready?<p className="safe-note" role="status">يتطلب حساب MT5 Demo متصلاً وحديثاً ووضع Expert أو Scalping أو Swing،
     وتقرير Core مطابقاً لنفس الوضع والحساب. القناص Sniper يتطلب بيانات Tick مستقلة ولا يجوز تقييمه هنا.</p>:<>
   <div className="operator-grid">
    <div>حالة الدراسة <strong>{active?'نشطة':state==='stopped'?'متوقفة':'لم تبدأ'}</strong></div>
    <div>شموع مستقبلية مقارنة <strong>{counter(data.bars_compared)}</strong></div>
    <div>قرارات في نفس الاتجاه <strong>{counter(data.same_direction)}</strong></div>
    <div>قرارات باتجاه مختلف <strong>{counter(data.different_direction)}</strong></div>
    {expert&&<>
     <div>أزواج Expert الورقية المكتملة <strong>{counter(paper.completed_pairs)}</strong></div>
     <div>نظيفة وتجاوزت التحقق التقريبي <strong>{counter(paper.clean_feasible_complete_pairs)}</strong></div>
     <div>أزواج بفجوات سعرية <strong>{counter(paper.quote_gap_affected_pairs)}</strong></div>
     <div>متوسط Frozen الورقي/لوت <strong>{amount(paper.clean_feasible_frozen_mean_net_per_lot)}</strong></div>
     <div>متوسط Adaptive الورقي/لوت <strong>{amount(paper.clean_feasible_adaptive_mean_net_per_lot)}</strong></div>
    </>}
   </div>
   <p className="safe-note">{expert?
    'نتائج Expert الورقية مشروطة بتغطية الأسعار وفحص لوت وهامش تقريبي؛ حتى المرور لا يعني تنفيذ الوسيط أو اختبار OOS مستقلاً.':
    'Scalping/Swing: مقارنة قرارات مستقبلية فقط، لا توجد صفقات افتراضية أو أرباح محسوبة لهذا الوضع، ولا اختبارات تنفيذ وسيط.'}</p>
  </>}
  <div className="quick-ops">
   <button type="button" disabled={disabled} onClick={onRefresh}>تحديث التجربة من Core</button>
   <button type="button" disabled={!canMutate||active} onClick={()=>void onAction('freeze')}>تجميد سياسة {title} على Demo</button>
   <button type="button" disabled={!canMutate||!active} onClick={()=>void onAction('stop')}>إيقاف التجربة البحثية</button>
  </div>
  <p className="safe-note">أزرار التجميد والإيقاف تحتاج STOP وأوامر وسيط مسوّاة في Rust Core.
   الإيقاف لا يغلق أي مركز MT5 ولا يوقف تسوية Expert الورقية المفتوحة.</p>
 </section>;
}
