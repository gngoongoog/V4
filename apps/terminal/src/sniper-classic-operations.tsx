import type {Status} from './types';

const obj=(v:unknown):Record<string,unknown>=>
 v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Record<string,unknown>:{};
const count=(v:unknown)=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=0?String(v):'—';
export function eligibleFrozenClassic(status:Status|null,page:unknown):boolean{
 const t=status?.telemetry,config=obj(obj(page).config),trading=obj(config.trading);
 return !!(status?.gateway_fresh&&t?.connected&&t.demo&&t.account_trade_mode===0
  &&Number.isSafeInteger(t.login)&&t.login>0&&t.server&&t.symbol&&t.currency
  &&trading.mode==='sniper'&&trading.sniper_brain==='classic');
}
export function SniperClassicOperations({status,page,report,preview,disabled,onRefresh,onAction}:{
 status:Status|null;page:unknown;report:unknown;preview:boolean;disabled:boolean;
 onRefresh:()=>void;onAction:(action:'freeze'|'stop')=>Promise<void>;
}){
 const t=status?.telemetry,view=obj(report);
 const eligible=!preview&&eligibleFrozenClassic(status,page);
 const expected=t?JSON.stringify([t.server,t.login,t.symbol,t.currency]):'';
 const scoped=eligible&&view.schema==='p1k5-classic-forward-v1'
   &&view.context===expected&&view.mode==='sniper'&&view.sniper_brain==='classic'
   &&view.no_broker_orders===true&&view.independent_oos===false
   &&view.broker_pnl===null&&view.paper_pnl===null
   &&(view.status==='active'||view.status==='stopped');
 const active=scoped&&view.status==='active';
 const ready=eligible&&status?.requested_run===false&&!disabled;
 return <section className="operator-card" aria-label="Sniper Classic prospective M1 research">
  <h3>Sniper Classic — رصد مستقبل مستقل P1-K5</h3>
  <p>Classic يعتمد على تحليل السوق وشموع M1 من اللقطة الكاملة، مو على تيكات
   Learner وحدها. هذا عدّ للإشارات الخام فقط، وليس دراسة صفقات مربحة أو Backtest.</p>
  {!scoped?<p role="status" className="safe-note">
    {eligible?'لا توجد تجربة Classic موثقة ومطابقة لهذا الحساب.':
     'للبدء اختر MT5 Demo حديث وSniper Classic ثم أوقف التنفيذ.'}
   </p>:<div className="operator-grid">
   <div>الحالة <strong>{active?'نشطة':'متوقفة'}</strong></div>
   <div>لقطات مستقبلية <strong>{count(view.prospective_full_snapshots)}</strong></div>
   <div>تقييمات Classic <strong>{count(view.evaluated_classic_snapshots)}</strong></div>
   <div>إشارات BUY <strong>{count(view.raw_buy_signals)}</strong></div>
   <div>إشارات SELL <strong>{count(view.raw_sell_signals)}</strong></div>
   <div>إشارات WAIT <strong>{count(view.raw_wait_signals)}</strong></div>
   <div>لقطات مستبعدة لفقد M1/Market <strong>{count(view.excluded_missing_m1_or_market)}</strong></div>
   <div>لقطات قديمة أو مكررة <strong>{count(view.excluded_old_or_duplicate)}</strong></div>
  </div>}
  <div className="quick-ops">
   <button type="button" disabled={preview||disabled} onClick={onRefresh}>تحديث من Core</button>
   <button type="button" disabled={!ready||active} onClick={()=>void onAction('freeze')}>بدء دراسة Classic</button>
   <button type="button" disabled={!ready||!active} onClick={()=>void onAction('stop')}>إيقاف دراسة Classic</button>
  </div>
  <p className="safe-note">دراسة منفصلة في ذاكرة Core؛ لا تُفتح صفقات ولا تُحدَّث
   أوزان العقول. لا تمثل OOS مستقلاً أو تغطية جميع التيكات أو نسبة نجاح.</p>
 </section>;
}
