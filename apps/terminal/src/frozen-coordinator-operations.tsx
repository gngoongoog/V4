import type {Status} from './types';

type Row={status?:unknown;paired_quote_paper?:unknown;bars_compared?:unknown;
 same_direction?:unknown;different_direction?:unknown;run_id?:unknown};
const object=(v:unknown):Record<string,unknown>=>
 v&&typeof v==='object'&&!Array.isArray(v)?v as Record<string,unknown>:{};
const count=(v:unknown):string=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=0?String(v):'—';
const amount=(v:unknown):string=>typeof v==='number'&&Number.isFinite(v)
 ?v.toLocaleString('en-US',{maximumFractionDigits:2}):'—';
export function frozenExpertEligible(status:Status|null,currentMode:unknown,context:unknown,armed:boolean):boolean{
 const t=status?.telemetry;
 return !!(status?.gateway_fresh && t?.connected && t.demo && t.account_trade_mode===0
    && t.login>0 && t.server && t.symbol && t.currency && currentMode==='expert'
    && context===JSON.stringify([t.server,t.login,t.symbol,t.currency]) && !armed);
}
export function FrozenCoordinatorOperations({status,source,currentMode,brainContext,armed,
 disabled,onRefresh,onAction}:{
 status:Status|null;source:unknown;currentMode:unknown;brainContext:unknown;
 armed:boolean;disabled:boolean;onRefresh:()=>void;
 onAction:(action:'freeze'|'stop')=>Promise<void>;
}){
 const eligible=frozenExpertEligible(status,currentMode,brainContext,armed);
 const data=object(source) as Row;
 const paper=object(data.paired_quote_paper);
 const state=typeof data.status==='string'?data.status:'unavailable';
 const ready=eligible && ['not_started','active','stopped'].includes(state);
 const active=ready && state==='active';
 return <section className="operator-card" aria-label="Frozen Expert coordinator prospective paper evidence">
  <h3>تقييم منسق Expert المجمّد — Frozen Research (P1-I)</h3>
  <p>المنسق البحثي يحتفظ بنسخة ثابتة من أوزان Expert وقراراته، ويقارنها بالمستشار المتكيّف
     على نفس اقتباس الأسعار المستقبلي. هذا تقييم ورقي؛ لا يتداول على MT5 ولا يغيّر العقول.</p>
  {!ready?<p className="safe-note" role="status">يتطلب حساب Demo مطابقاً وبيانات حديثة ووضع Expert، مع STOP قبل التجميد والإيقاف. أو أن تقرير التجربة لم يصل من Core.</p>:<>
   <div className="operator-grid">
    <div>حالة التجربة <strong>{active?'نشطة':state==='stopped'?'متوقفة':'لم تبدأ'}</strong></div>
    <div>قرارات متطابقة المصدر <strong>{count(data.bars_compared)}</strong></div>
    <div>أزواج ورقية مكتملة <strong>{count(paper.completed_pairs)}</strong></div>
    <div>نتائج نظيفة ومبدئياً قابلة للتنفيذ <strong>{count(paper.clean_feasible_complete_pairs)}</strong></div>
    <div>متأثرة بفجوات الأسعار <strong>{count(paper.quote_gap_affected_pairs)}</strong></div>
    <div>تغطية غير موثّقة <strong>{count(paper.unknown_quote_gap_evidence_pairs)}</strong></div>
    <div>مؤشر Frozen النظري لكل لوت <strong>{amount(paper.clean_feasible_frozen_mean_net_per_lot)}</strong></div>
    <div>مؤشر Adaptive النظري لكل لوت <strong>{amount(paper.clean_feasible_adaptive_mean_net_per_lot)}</strong></div>
   </div>
   <p className="safe-note">حالة المقارنة: {paper.status==='descriptive_only'?'وصفية فقط':paper.status==='insufficient_clean_feasible_pairs'?'عينات نظيفة قليلة أو شروط الوسيط غير مؤكدة':'لا توجد 30 صفقة ورقية مكتملة'}.
   نتيجة شرط اللوت والهامش تقريبية فقط، وليست قبول أمر من الوسيط. لا نقارن هنا Sniper أو Scalping أو Swing ولا ندّعي ربحاً فعلياً.</p>
  </>}
  <div className="quick-ops">
   <button type="button" onClick={onRefresh} disabled={disabled}>تحديث التجربة من Core</button>
   <button type="button" disabled={!ready||active||disabled} onClick={()=>void onAction('freeze')}>تجميد سياسة Expert على Demo</button>
   <button type="button" disabled={!active||disabled} onClick={()=>void onAction('stop')}>إيقاف التجربة البحثية</button>
  </div>
  <p className="safe-note">STOP يوقف الدخول الجديد للنظام؛ زر إيقاف التجربة يوقف جمع الإشارات الجديدة فقط ويترك تسوية النتائج الورقية المفتوحة. لا يتدخل أي منهما في إدارة مراكز الوسيط.</p>
 </section>;
}
