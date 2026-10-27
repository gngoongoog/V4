import {fourModeObservation} from './mode-evidence';
import type {Status} from './types';

const labels:Record<string,string>={
 sniper:'القناص Sniper',scalping:'سكالبينغ Scalping',
 swing:'السوينغ Swing',expert:'الخبير Expert'
};
const problems:Record<string,string>={
 cross_mode_position_collision:'صفقة منسوبة لأكثر من وضع',
 malformed_account_broker_evidence:'سجل وسيط تالف',
 invalid_or_duplicate_broker_samples:'نتائج ناقصة أو مكررة',
 insufficient_closed_demo_deals:'أقل من 30 إغلاق Demo موثّق',
 mixed_or_unverified_strategy_sources:'تعدّد نسخ استراتيجية غير متجانسة',
 descriptive_evidence_only:'دليل وصفي فقط، ليس اختبار OOS مستقلاً'
};
const fmt=(v:number|null,decimal=2)=>v===null?'—':v.toLocaleString('en-US',{maximumFractionDigits:decimal});
export function ModeEvidenceOperations({status,report,disabled,onRefresh}:{
 status:Status|null;report:unknown;disabled:boolean;onRefresh:()=>void;
}){
 const view=fourModeObservation(status,report,Date.now());
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
  <p className="safe-note">عرض كل وضع لا يزيد عن أحدث 240 عينة موثقة. حتى 30 صفقة لا تثبت الأفضلية المستقبلية، والمقارنة بين الأوضاع ليست متكافئة زمنياً أو في ظروف السوق إلا بتجربة مستقلة مجمدة.</p>
  <div className="quick-ops"><button type="button" disabled={disabled} onClick={onRefresh}>تحديث أدلة الأوضاع من Rust Core</button></div>
 </section>;
}
