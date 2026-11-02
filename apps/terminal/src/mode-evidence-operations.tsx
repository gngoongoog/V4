import {fourModeObservation} from './mode-evidence';
import type {Status} from './types';

const labels:Record<string,string>={
 sniper:'القناص Sniper',scalping:'سكالبينغ Scalping',
 swing:'السوينغ Swing',expert:'الخبير Expert'
};
const modes=['sniper','scalping','swing','expert'];
const shadowState:Record<string,string>={
 tick_native_not_supported:'يتطلب فحص تيكات القناص؛ لا نختلق تدريب شموع',
 no_prospective_samples:'لا توجد عينات Forward مسجلة',
 recent_historical_observation:'عينات بحثية سابقة حديثة',
 historical_observation_stale:'عينات بحثية سابقة قديمة'
};
const problems:Record<string,string>={
 cross_mode_position_collision:'صفقة منسوبة لأكثر من وضع',
 malformed_account_broker_evidence:'سجل وسيط تالف',
 invalid_or_duplicate_broker_samples:'نتائج ناقصة أو مكررة',
 insufficient_closed_demo_deals:'أقل من 30 إغلاق Demo موثّق',
 mixed_or_unverified_strategy_sources:'تعدّد نسخ استراتيجية غير متجانسة',
 descriptive_evidence_only:'دليل وصفي فقط، ليس اختبار OOS مستقلاً'
};
const horizons:Record<string,string>={
 tick_native_not_studied:'تيكات Sniper — غير مدروسة هنا',
 '12_completed_M1_bars_approx_12_minutes':'12 شمعة M1 (~12 دقيقة)',
 '12_completed_H4_bars_approx_48_hours':'12 شمعة H4 (~48 ساعة)',
 '12_completed_advisor_timeframe_bars':'12 شمعة من فريم المستشار'
};
const fmt=(v:number|null,decimal=2)=>v===null?'—':v.toLocaleString('en-US',{maximumFractionDigits:decimal});
export function ModeEvidenceOperations({status,report,disabled,onRefresh}:{
 status:Status|null;report:unknown;disabled:boolean;onRefresh:()=>void;
}){
 const view=fourModeObservation(status,report,Date.now());
 const raw=report!==null&&typeof report==='object'&&!Array.isArray(report)
  ?report as Record<string,unknown>:{};
 const shadow=raw.inactive_forward!==null&&typeof raw.inactive_forward==='object'&&!Array.isArray(raw.inactive_forward)
  ?raw.inactive_forward as Record<string,unknown>:{};
 const shadowRows=view.available&&shadow.source==='prospective_inactive_mode_candle_forward_only'&&
  shadow.broker_orders===false&&shadow.can_promote_to_live===false&&Array.isArray(shadow.modes)
  ?shadow.modes.filter((entry:unknown)=>{
   if(entry===null||typeof entry!=='object'||Array.isArray(entry))return false;
   const r=entry as Record<string,unknown>;
   return modes.includes(String(r.mode))&&r.broker_orders===false&&r.demo_broker_fills===false&&
    r.independent_frozen_oos===false&&typeof r.samples==='number'&&Number.isInteger(r.samples)&&r.samples>=0&&
    typeof r.scored_after_future_bars==='number'&&Number.isInteger(r.scored_after_future_bars)&&
    r.scored_after_future_bars>=0&&r.scored_after_future_bars<=r.samples&&
    typeof r.rejected_or_unverifiable_scored==='number'&&
    Number.isInteger(r.rejected_or_unverifiable_scored)&&
    r.rejected_or_unverifiable_scored>=0&&
    r.scored_after_future_bars+r.rejected_or_unverifiable_scored<=r.samples&&
    r.scoring_validation==='same_causal_cost_horizon_validator_as_learning_core'&&
    typeof r.horizon==='string'&&typeof horizons[r.horizon]==='string';
  }) as Record<string,unknown>[]:[];
 return <section className="operator-card" aria-label="Four trading mode broker evidence">
  <h3>أدلة الأوضاع الأربعة — إغلاقات MT5 Demo بعد التكاليف</h3>
  <p>تقرير المقارنة هذا يقرأ صفقات Demo المصالحة والمنسوبة للمنسق الرئيسي فقط؛
     لا يستخدم نتائج Paper أو Shadow، ولا يمنح حق التشغيل على REAL.
     نتائج التعلم المستمر السابقة ليست اختباراً مستقلاً خارج العينة (OOS).</p>
  {view.available?<div>
   <div className="operator-grid">
    <div>صفقة منسوبة لأكثر من وضع <strong>{view.collisions}</strong></div>
    <div>سجلات وسيط تالفة <strong>{view.malformed}</strong></div>
   </div>
   <div className="table-scroll"><table>
    <thead><tr>{['الوضع','Demo مغلقة','الدليل','نسبة الفوز بعد التكاليف','الصافي المحجوز','سبب التقييد'].map(x=><th key={x}>{x}</th>)}</tr></thead>
    <tbody>{view.rows.map(row=><tr key={row.mode}>
     <td>{labels[row.mode]}</td>
     <td>{row.samples}</td>
     <td>{row.state==='descriptive_only'?'وصفي فقط':'غير كافٍ أو متعارض'}</td>
     <td>{row.winRate===null?'—':fmt(row.winRate*100,1)+'%'}</td>
     <td>{fmt(row.net)}</td>
     <td>{problems[row.reason]||'الدليل غير كافٍ'}</td>
    </tr>)}</tbody>
   </table></div>
  </div>:<p className="safe-note" role="status">{view.reason}</p>}
  {view.available&&<div>
   <h4>مراقبة Forward للأوضاع غير النشطة — بدون صفقات MT5</h4>
   <p className="safe-note">هذه عينات تجريبية من الشموع المكتملة لحساب Demo المطابق، تُقيّم بعد وصول شموع لاحقة.
      النتائج المقبولة مرّت بتدقيق الأفق الزمني والشموع المتتابعة واحتساب المخاطر والتكاليف داخل Core.
      لا تُضاف لإغلاقات الوسيط، ولا تمثل تعلّم Sniper بالتيك، ولا اختبار OOS مجمداً.
      لا تُنتج أوامر تداول أو تفويض تشغيل REAL.</p>
   {shadowRows.length===4?<div className="table-scroll"><table>
    <thead><tr>{['الوضع','العينات المستقبلية','نتائج تحقّق منها Core','نتائج مرفوضة','نافذة الدراسة','الحالة'].map(x=><th key={x}>{x}</th>)}</tr></thead>
    <tbody>{shadowRows.map(row=><tr key={String(row.mode)}>
     <td>{labels[String(row.mode)]||String(row.mode)}</td>
     <td>{Number(row.samples)}</td>
     <td>{Number(row.scored_after_future_bars)}</td>
     <td>{Number(row.rejected_or_unverifiable_scored)}</td>
     <td>{horizons[String(row.horizon)]}</td>
     <td>{shadowState[String(row.status)]||'غير مؤكدة'}</td>
    </tr>)}</tbody>
   </table></div>:<p className="safe-note">لا تتوفر بيانات Forward موثقة للأوضاع غير النشطة من Core بعد.</p>}
  </div>}
  <p className="safe-note">عرض كل وضع لا يزيد عن أحدث 240 عينة موثقة. حتى 30 صفقة لا تثبت الأفضلية المستقبلية، والمقارنة بين الأوضاع ليست متكافئة زمنياً أو في ظروف السوق إلا بتجربة مستقلة مجمدة.</p>
  <div className="quick-ops"><button type="button" disabled={disabled} onClick={onRefresh}>تحديث أدلة الأوضاع من Rust Core</button></div>
 </section>;
}
