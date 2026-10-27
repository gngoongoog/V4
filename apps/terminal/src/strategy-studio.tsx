import {useState} from 'react';
import type {Status} from './types';
export type StudioAction='run'|'auto_on'|'auto_off'|'cancel'|'adopt_demo';
const obj=(v:unknown):Record<string,unknown>=>v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Record<string,unknown>:{};
const rows=(v:unknown):Record<string,unknown>[]=>Array.isArray(v)?v.map(obj):[];
const num=(v:unknown)=>typeof v==='number'&&Number.isFinite(v)?v.toLocaleString('en-US',{maximumFractionDigits:3}):'—';
const text=(v:unknown)=>typeof v==='string'?v:'—';
const reasons=(v:unknown)=>Array.isArray(v)?v.filter((x):x is string=>typeof x==='string').map(x=>{
 const phase=x.startsWith('training_')?'التدريب':x.startsWith('validation_')?'التحقق':'الاختبار النهائي';
 return phase+': '+(x.endsWith('insufficient_trades')?'العينة غير كافية':x.endsWith('nonpositive_after_costs')?'النتيجة غير موجبة بعد التكاليف':x.endsWith('invalid_metrics')?'مقاييس غير صالحة':x);
}):[];
export function StrategyStudioOperations({source,status,disabled,onAction,onRefresh}:{source:unknown;status:Status|null;disabled:boolean;
 onAction:(action:StudioAction,context:string,value?:string)=>Promise<void>;onRefresh:()=>void}){
 const [timeframe,setTimeframe]=useState('M1');const [reportId,setReportId]=useState('');
 const data=obj(source),t=status?.telemetry;
 const context=t?JSON.stringify([t.server,t.login,t.symbol,t.currency]):'';
 const matched=!!t&&status?.gateway_fresh===true&&t.connected&&data.connected===true&&data.context===context;
 const reports=matched?rows(data.reports):[];
 const entry=reports.find(v=>v.id===reportId)||reports[0],report=obj(entry?.report),evolution=obj(report.evolution);
 const trials=rows(evolution.trials),selected=trials.find(v=>v.id===evolution.selected_trial);
 const reject=reasons(evolution.rejection_reasons),active=entry?.id===data.active_report_id;
 const canAdopt=matched&&t?.demo===true&&t.account_trade_mode===0&&!status?.requested_run&&data.trading_mode==='expert'&&report.eligible_for_review===true&&!active;
 const action=(name:StudioAction,value?:string)=>{if(!disabled&&(matched||name==='cancel'))void onAction(name,context,value)};
 return <section className="operator-card" aria-label="Strategy evolution laboratory">
 <h3>مختبر تطوير الاستراتيجيات</h3>
 <p>بحث بقواعد قابلة للفحص: توليد بدائل ← تعديل على التدريب ← اختيار بالتحقق ← اختبار نهائي منفصل. النتائج هنا محاكاة شموع تشمل السبريد والعمولة والانزلاق؛ لا تشمل السواب وليست أرباح MT5.</p>
 <div className="quick-ops">
  <label>الفريم <select aria-label="فريم البحث" value={timeframe} onChange={e=>setTimeframe(e.target.value)}>
   <option value="ALL">كل الفريمات المتاحة</option>{(matched?rows(data.frames):[{timeframe:'M1'}]).map(f=><option key={text(f.timeframe)} value={text(f.timeframe)}>{text(f.timeframe)} · {num(f.bars)} شمعة</option>)}
  </select></label>
  <button disabled={disabled||!matched} onClick={()=>action('run',timeframe)}>تشغيل دورة تطوير</button>
  <button disabled={disabled||!matched} onClick={()=>action(data.auto===true?'auto_off':'auto_on')}>{data.auto===true?'إيقاف البحث التلقائي':'بحث تلقائي مع بيانات جديدة'}</button>
  <button disabled={disabled} onClick={()=>action('cancel')}>إلغاء البحث الجاري والطابور</button>
  <button disabled={disabled} onClick={onRefresh}>تحديث النتائج</button>
 </div>
 {!matched?<p className="safe-note">يلزم اتصال MT5 حديث لنفس الحساب لعرض البحث وتشغيله. لا توجد نتائج موثقة للعرض الآن.</p>:<>
 <p className="safe-note">{data.auto===true?'البحث التلقائي مفعّل لهذا الحساب':'البحث التلقائي متوقف'} · يعمل: {text(data.working)} · في الانتظار: {num(data.queued)}</p>
 {typeof data.last_error==='string'&&data.last_error&&<p role="alert">خطأ البحث: {data.last_error}</p>}
 {!entry?<p>لم تُحفظ دورة بعد. يلزم 400 شمعة مغلقة على الأقل واختبار نهائي جديد لم يُستخدم سابقاً.</p>:<>
 <label>دورة البحث <select aria-label="دورة البحث" value={text(entry.id)} onChange={e=>setReportId(e.target.value)}>{reports.map(v=><option key={text(v.id)} value={text(v.id)}>{text(obj(v.report).timeframe)} · {typeof v.created_ms==='number'?new Date(v.created_ms).toLocaleString('ar-IQ'):text(v.id)}</option>)}</select></label>
 <div className="operator-grid"><div>التركيبات المختبرة<strong>{num(report.trials)}</strong></div><div>الأجيال<strong>{num(evolution.generations)}</strong></div><div>العائلة المختارة<strong>{text(obj(report.selected).family)}</strong></div><div>الدورة السابقة<strong>{text(evolution.parent_run_id)}</strong></div></div>
 <div className="table-scroll"><table><thead><tr><th>العينة</th><th>الصفقات المحاكية</th><th>الصافي R</th><th>أقصى تراجع R</th><th>التكاليف لكل لوت</th></tr></thead><tbody>{[['training','التدريب'],['validation','التحقق'],['test','الاختبار النهائي']].map(([key,label])=>{const m=obj(report[key]);return <tr key={key}><th>{label}</th><td>{num(m.trades)}</td><td>{num(m.net_r)}</td><td>{num(m.max_drawdown_r)}</td><td>{num(m.costs_per_lot)}</td></tr>})}</tbody></table></div>
 <p className="safe-note">{report.eligible_for_review===true?'مرشّح للمراجعة التجريبية؛ لا يكفي لإثبات الربحية المستقبلية.':'المرشح غير مؤهل للاعتماد.'} {reject.join(' · ')}</p>
 <p>أفضل محاولة تدريب: {num(evolution.best_training_trial)} · أضعف محاولة تدريب: {num(evolution.worst_training_trial)} · المختارة بعد التحقق: {num(selected?.id)}. الأفضل تاريخياً قد يفشل لاحقاً؛ لا تُنقل أرباح المحاكاة إلى سجل الوسيط.</p>
 <button disabled={disabled||!canAdopt} onClick={()=>action('adopt_demo',text(entry.id))}>{active?'معتمد حالياً كمستشار Demo':'اعتماد المرشح كمستشار Expert Demo'}</button>
 <p className="safe-note">الاعتماد يتطلب STOP وحساب Demo مربوطاً دون صفقات أو أوامر معلقة ووضع Expert. لا يرسل الزر صفقة؛ بعد START يبقى المنسق صاحب التنفيذ. نتائج Demo المغلقة الموثقة تحدّث وزن هذا التقرير وحده. لا توجد ترقية تلقائية إلى REAL.</p>
 <details><summary>المحاولات والتحسينات والأفضل والأسوأ</summary>{trials.length?<div className="table-scroll"><table><thead><tr><th>المحاولة / الأصل</th><th>الجيل</th><th>العائلة / النظر للخلف</th><th>وقف ATR / الهدف R</th><th>تقييم التدريب</th><th>تقييم التحقق</th></tr></thead><tbody>{trials.map(v=><tr key={num(v.id)}><td>{num(v.id)} / {num(v.parent)}</td><td>{num(v.generation)}</td><td>{text(obj(v.genome).family)} / {num(obj(v.genome).lookback)}</td><td>{num(obj(v.genome).stop_atr)} / {num(obj(v.genome).reward)}</td><td>{num(v.training_score)}</td><td>{num(v.validation_score)}</td></tr>)}</tbody></table></div>:<p>تقرير قديم: سجل الأجيال غير متاح؛ لا نولّد له بيانات بأثر رجعي.</p>}</details>
 </>}
 </>}
 </section>;
}
