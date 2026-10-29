import {useEffect,useRef,useState} from 'react';
import type {FormEvent} from 'react';
import {Archive,CalendarClock,Download,Play,Square,ShieldAlert} from 'lucide-react';

const iso=(offsetDays:number)=>new Date(Date.now()+offsetDays*86400000).toISOString().slice(0,10);

export function BacktestOperations({onStart,onCancel,disabled,active}: {
  onStart:(input:{from_date:string;to_date:string;initial_equity:number;spread_floor:number})=>Promise<void>;
  onCancel:()=>Promise<void>;disabled:boolean;active:boolean;
}) {
 const [from,setFrom]=useState(iso(-9));
 const [to,setTo]=useState(iso(-1));
 const [equity,setEquity]=useState('1000');
 const [spread,setSpread]=useState('0.2');
 const [error,setError]=useState('');
 const submit=async(e:FormEvent)=>{
  e.preventDefault();
  const initial_equity=Number(equity),spread_floor=Number(spread);
  if(from>=to||to>iso(0)||!Number.isFinite(initial_equity)||initial_equity<=0||!Number.isFinite(spread_floor)||spread_floor<0){
    setError('اختر فترة تاريخية صحيحة ورصيداً موجباً وسبريد غير سالب');return;
  }
  setError('');await onStart({from_date:from,to_date:to,initial_equity,spread_floor});
 };
 return <form className="operator-card" onSubmit={e=>void submit(e)}>
   <h3><CalendarClock size={18}/> اختبار تاريخي حقيقي من بيانات MT5</h3>
   <p>يطلب المحرك شموع الوسيط المكتملة ويطبق الاستراتيجية والتكاليف؛ النتائج ليست صفقات حقيقية ولا ضمان ربح.</p>
   <div className="operator-grid">
    <label>من UTC<input aria-label="backtest from" type="date" value={from} max={to} required onChange={e=>setFrom(e.target.value)}/></label>
    <label>إلى UTC (غير شامل)<input aria-label="backtest to" type="date" value={to} min={from} max={iso(0)} required onChange={e=>setTo(e.target.value)}/></label>
    <label>الرصيد الافتراضي<input aria-label="initial equity" type="number" min="1" step="any" required value={equity} onChange={e=>setEquity(e.target.value)}/></label>
    <label>أقل سبريد مسموح بالحساب<input aria-label="spread floor" type="number" min="0" step="0.01" required value={spread} onChange={e=>setSpread(e.target.value)}/></label>
   </div>
   {error&&<div className="operator-error" role="alert">{error}</div>}
   <div className="quick-ops"><button type="submit" disabled={disabled||active}><Play size={16}/> بدء الاختبار من Core</button><button type="button" disabled={disabled} onClick={()=>void onCancel()}><Square size={16}/> إلغاء الاختبار</button></div>
   <p className="safe-note">لبدء اختبار جديد: STOP أولاً، اتصال MT5 حديث، وعدم وجود صفقة بحثية مفتوحة. يستطيع Core رفض الطلب إذا لم تتحقق الشروط.</p>
  </form>;
}
export function BackupOperations({create,exportBrain,disabled}: {create:()=>Promise<void>;exportBrain:()=>Promise<void>;disabled:boolean}) {
 return <section className="operator-card" aria-label="Backup operations">
  <h3><Archive size={18}/> حماية قاعدة البيانات والعقول</h3>
  <p>تُنشأ النسخة داخل مجلد بيانات BRAINTRADE عبر Rust Core بعد التحقق من قاعدة البيانات. الأسرار وسجل إرسال الأوامر لا يُستعادان ضمن Brain Snapshot.</p>
  <div className="quick-ops"><button disabled={disabled} onClick={()=>void create()}><Archive size={16}/> إنشاء Backup موثّق</button><button disabled={disabled} onClick={()=>void exportBrain()}><Download size={16}/> تصدير Brain Snapshot</button></div>
  <div className="safe-note"><ShieldAlert size={14}/> الاستعادة ليست زرّاً سريعاً أثناء التداول؛ تحتاج إيقاف التنفيذ والتحقق من الملفات والأوامر أولاً.</div>
 </section>;
}

const MODES=[['scalping','Scalping'],['swing','Swing'],['expert','Expert'],['sniper','Sniper']] as const;
const STYLES=['auto','impulse','rebound','confirmed'];
const safeObject=(data:unknown):Record<string,unknown>=>data!==null&&typeof data==='object'&&!Array.isArray(data)?data as Record<string,unknown>:{};

export function TradingSettingsOperations({source,disabled,armed,apply,brokerPoint}: {
 source:unknown;disabled:boolean;armed:boolean;apply:(input:Record<string,unknown>)=>Promise<void>;brokerPoint?:number|null;
}) {
 const view=safeObject(source);
 const stored=safeObject(safeObject(view.config).trading);
 const commission=safeObject(view.commission_editor);
 const [draft,setDraft]=useState<Record<string,unknown>>({});const [saving,setSaving]=useState(false);
 const [fixedPips,setFixedPips]=useState<string|null>(null);
 const [fee,setFee]=useState('7');const [basis,setBasis]=useState('round_trip');const [error,setError]=useState('');
 const [editedFee,setEditedFee]=useState(false);const [editedBasis,setEditedBasis]=useState(false);
 // A draft belongs to the exact settings, fees, account and slippage that the
 // operator reviewed. Background refreshes must neither discard the draft nor
 // permit it to overwrite a newer revision without an explicit reset.
 const sourceId=JSON.stringify([stored,commission.amount,commission.basis,commission.account,
   safeObject(safeObject(view.config).risk).slippage_ticks]);
 const editSource=useRef<string|null>(null);
 const markEditing=()=>{if(editSource.current===null)editSource.current=sourceId};
 const stale=editSource.current!==null&&editSource.current!==sourceId;
 const edit={...stored,...draft};
 const patch=(key:string,value:unknown)=>{markEditing();setDraft(old=>({...old,[key]:value}))};
 const mode=String(edit.mode||'expert'),priority=edit.priority_enabled===true;
 const fallback=String(edit.fallback_mode||'expert');
 const sniper=edit.sniper_active===true;
 const numeric=(key:string,fallback:number)=>Number(edit[key]??fallback);
 // Preserve partial numeric input until save; never feed NaN back into React number fields.
 const numericPatch=(key:string,value:string)=>patch(key,value);
 const numericInput=(key:string,fallback:number)=>String(edit[key]??fallback);
 const locked=disabled||armed||saving;
 const save=async()=>{
  if(stale){setError('تغيّرت إعدادات Core أو الحساب أثناء التعديل. المسودة محفوظة؛ راجع قيمك ثم تراجع عن التعديلات وأعد إدخالها قبل الحفظ.');return}
  const updated={...stored,...draft};
  if(updated.priority_enabled===true){updated.mode=updated.sniper_active===true?'sniper':String(updated.fallback_mode||'expert');}
  if(updated.fallback_mode==='sniper'){setError('الوضع البديل لا يمكن أن يكون قناصاً');return}
  const rate=Number(editedFee?fee:commission.amount??7);
  if((editedFee&&fee.trim()==='')||!Number.isFinite(rate)||rate<0||rate>1000){setError('العمولة يجب أن تكون بين 0 و1000 لكل لوت');return}
  const bounded:[string,number,number][]=[
   ['sniper_target_points',1,100000],['sniper_stop_points',1,100000],
   ['sniper_cooldown_seconds',0,3600],['sniper_max_hold_seconds',1,3600],
   ['sniper_min_target_pips',0.1,1000],['sniper_max_target_pips',0.1,1000],
   ['sniper_pip_price',0.00001,1000],['max_cost_fraction',0.01,0.9],
    ['pip_points',1,1000]
  ];
  for(const [key,min,max] of bounded){
   const raw=updated[key];
   const value=Number(raw);
   if(raw===null||raw===undefined||(typeof raw==='string'&&raw.trim()==='')||!Number.isFinite(value)||value<min||value>max
     ||(['sniper_cooldown_seconds','sniper_max_hold_seconds'].includes(key)&&!Number.isInteger(value))){
    setError('قيمة القناص غير صالحة: '+key+' (من '+min+' إلى '+max+')');return;
   }
   updated[key]=value;
  }
  if(Number(updated.sniper_max_target_pips)<=Number(updated.sniper_min_target_pips)){
   setError('الهدف المتكيف الأعلى يجب أن يكون أكبر من الأدنى');return;
  }
  if(updated.mode==='sniper'&&typeof brokerPoint==='number'&&Number.isFinite(brokerPoint)&&brokerPoint>0){
   const points=Number(updated.pip_points),actual=Number(updated.sniper_pip_price)/brokerPoint;
   if(Math.abs(actual-points)>Math.max(1e-6,Math.abs(points)*1e-7)){
    setError('عدد نقاط MT5 لكل pip لا يطابق سعر pip ونقطة الوسيط الحالية. عدّل القيم قبل الحفظ.');return;
   }
  }
  if(!['fixed','adaptive'].includes(String(updated.sniper_target_mode))){
   setError('نوع هدف القناص غير مدعوم');return;
  }
  const account=safeObject(commission.account);
  const payload:Record<string,unknown>={trading:updated,commission_per_lot:rate,
   commission_basis:editedBasis?basis:String(commission.basis||'round_trip'),
   slippage_ticks:Number(safeObject(safeObject(view.config).risk).slippage_ticks??2)};
  // Bind fees to the account the operator actually reviewed; never silently
  // switch commission profiles during a delayed save.
  if(!['round_trip','per_side'].includes(String(payload.commission_basis))){setError('طريقة احتساب العمولة غير مدعومة');return;}
  if(Object.keys(account).length)payload.account=account;
  setError('');setSaving(true);
  try{
   await apply(payload);
   // Only discard the draft after Core has re-read and confirmed persistence.
   editSource.current=null;setDraft({});setEditedFee(false);setEditedBasis(false);setFixedPips(null);
  }catch(e){setError('فشل الحفظ؛ تم الاحتفاظ بالتعديلات: '+String(e))}
  finally{setSaving(false)}
 };
 // The editor retains the user's typing; a new Core revision is not silently
 // applied on top of unsaved fields. Clicking reset re-reads the current scope.
 const reset=()=>{editSource.current=null;setDraft({});setEditedFee(false);setEditedBasis(false);setFixedPips(null);setError('')};
 return <section className="operator-card" aria-label="Coordinator and costs">
  <h3><ShieldAlert size={18}/> منسق التداول وتكاليف الوسيط</h3>
  <p>يوجد منسق واحد فقط لإرسال صفقات MT5. العقول الأخرى تحليلية. لا يمكن تعديل هذه الإعدادات أثناء START.</p>
  <div className="operator-grid">
   <label>وضع تنفيذ الصفقات<select value={mode} disabled={priority||locked} onChange={e=>patch('mode',e.target.value)}>{MODES.map(([v,label])=><option key={v} value={v}>{label}</option>)}</select></label>
   <label>الوضع البديل للقناص<select value={fallback} disabled={locked} onChange={e=>patch('fallback_mode',e.target.value)}>{MODES.filter(([v])=>v!=='sniper').map(([v,label])=><option key={v} value={v}>{label}</option>)}</select></label>
   <label className="operator-toggle"><input type="checkbox" checked={priority} disabled={locked} onChange={e=>{markEditing();setDraft(old=>({...old,priority_enabled:e.target.checked,mode:e.target.checked?(sniper?'sniper':fallback):mode}))}}/> تفعيل أولوية القناص</label>
   <label className="operator-toggle"><input type="checkbox" checked={sniper} disabled={!priority||locked} onChange={e=>{markEditing();setDraft(old=>({...old,sniper_active:e.target.checked,mode:e.target.checked?'sniper':fallback}))}}/> القناص هو المنسق النشط</label>
   <label>أسلوب القناص<select value={String(edit.sniper_style||'auto')} disabled={locked} onChange={e=>patch('sniper_style',e.target.value)}>{STYLES.map(s=><option key={s} value={s}>{s}</option>)}</select></label>
   <label>عقل القناص<select value={String(edit.sniper_brain||'classic')} disabled={locked} onChange={e=>patch('sniper_brain',e.target.value)}><option value="classic">Classic</option><option value="learner">Learner</option></select></label>
   <label>العمولة لكل 1 لوت<input aria-label="commission per lot" type="number" min="0" step="0.01" disabled={locked} value={editedFee?fee:String(commission.amount??7)} onChange={e=>{markEditing();setEditedFee(true);setFee(e.target.value)}}/></label>
   <label>طريقة احتساب العمولة<select aria-label="commission basis" disabled={locked} value={editedBasis?basis:String(commission.basis||'round_trip')} onChange={e=>{markEditing();setEditedBasis(true);setBasis(e.target.value)}}><option value="round_trip">دخول وخروج معاً</option><option value="per_side">لكل جهة (مرتين)</option></select></label>
  </div>
  <h4>إعدادات القناص المنفّذة لدى Rust Core</h4>
  <p className="safe-note">تُحفظ القيم الفعلية للقناص مع وضع التداول. الهدف الثابت يُعرض بوحدة pip (المحرك يحفظه كنقاط داخلياً)؛ وقف الخسارة بوحدة نقاط تسعير MT5.</p>
  <div className="operator-grid">
   <label>الهدف الثابت (pip)<input aria-label="sniper fixed target pips" type="number" min="0.1" step="0.1" value={fixedPips??String(numeric('sniper_target_points',50)/numeric('pip_points',10))} disabled={locked} onChange={e=>{const value=e.target.value;setFixedPips(value);numericPatch('sniper_target_points',value===''?'':String(Number(value)*numeric('pip_points',10)))}}/></label>
   <label>وقف الخسارة (MT5 points)<input aria-label="sniper stop points" type="number" min="1" step="1" value={numericInput('sniper_stop_points',80)} disabled={locked} onChange={e=>numericPatch('sniper_stop_points',e.target.value)}/></label>
   <label>نوع هدف القناص<select aria-label="sniper target mode" value={String(edit.sniper_target_mode||'fixed')} disabled={locked} onChange={e=>patch('sniper_target_mode',e.target.value)}><option value="fixed">ثابت</option><option value="adaptive">متكيّف (Demo فقط)</option></select></label>
   <label>أدنى هدف متكيّف (pip)<input aria-label="sniper adaptive minimum pips" type="number" min="0.1" step="0.1" value={numericInput('sniper_min_target_pips',2)} disabled={locked} onChange={e=>numericPatch('sniper_min_target_pips',e.target.value)}/></label>
   <label>أعلى هدف متكيّف (pip)<input aria-label="sniper adaptive maximum pips" type="number" min="0.1" step="0.1" value={numericInput('sniper_max_target_pips',10)} disabled={locked} onChange={e=>numericPatch('sniper_max_target_pips',e.target.value)}/></label>
   <label>الفاصل بين دخولين (ثانية)<input aria-label="sniper cooldown seconds" type="number" min="0" max="3600" step="1" value={numericInput('sniper_cooldown_seconds',0)} disabled={locked} onChange={e=>numericPatch('sniper_cooldown_seconds',e.target.value)}/></label>
   <label>أقصى احتفاظ بالصفقة (ثانية)<input aria-label="sniper max hold seconds" type="number" min="1" max="3600" step="1" value={numericInput('sniper_max_hold_seconds',120)} disabled={locked} onChange={e=>numericPatch('sniper_max_hold_seconds',e.target.value)}/></label>
   <label>عدد نقاط MT5 لكل pip<input aria-label="sniper points per pip" type="number" min="1" max="1000" step="any" value={numericInput('pip_points',10)} disabled={locked} onChange={e=>{markEditing();const value=e.target.value;const pips=fixedPips??String(numeric('sniper_target_points',50)/numeric('pip_points',10));setFixedPips(pips);setDraft(old=>({...old,pip_points:value,sniper_target_points:value===''||pips===''?'':String(Number(value)*Number(pips))}))}}/></label>
   <label>حجم pip بسعر الذهب<input aria-label="sniper pip price" type="number" min="0.00001" step="0.01" value={numericInput('sniper_pip_price',0.1)} disabled={locked} onChange={e=>numericPatch('sniper_pip_price',e.target.value)}/></label>
   <label>حصة التكلفة القصوى من الهدف (0.01–0.9)<input aria-label="sniper maximum cost fraction" type="number" min="0.01" max="0.9" step="0.01" value={numericInput('max_cost_fraction',0.45)} disabled={locked} onChange={e=>numericPatch('max_cost_fraction',e.target.value)}/></label>
   <label className="operator-toggle"><input type="checkbox" checked={edit.demo_flexible===true} disabled={locked} onChange={e=>patch('demo_flexible',e.target.checked)}/> مرونة تنفيذ Demo</label>
   <label className="operator-toggle"><input type="checkbox" checked={edit.adaptive_exits===true} disabled={locked} onChange={e=>patch('adaptive_exits',e.target.checked)}/> تعلّم مخارج Demo</label>
  </div>
  <p className="safe-note">إذا كانت العمولة 7 دولار لكل لوت ذهاباً وإياباً: حجم 0.01 = $0.07، و0.1 = $0.70، و1.0 = $7.00. هذا تقدير تكلفة، والخصم الفعلي من شركة الوساطة.</p>
  {error&&<div role="alert" className="operator-error">{error}</div>}
  {stale&&<div role="alert" className="operator-error">تغيّرت إعدادات Core أو الحساب؛ المسودة محفوظة لكن الحفظ متوقف حتى مراجعتها وإعادة إدخالها.</div>}
  <div className="quick-ops"><button disabled={locked||!Object.keys(stored).length} onClick={()=>void save()}>حفظ الوضع والعمولة لدى Core</button><button disabled={disabled||saving} onClick={reset}>التراجع عن التعديلات</button></div>
  <small>إصدار المصدر: {Object.keys(stored).length?'إعدادات Core':'غير متاح'}</small>
 </section>;
}

export function BrainControlsOperations({source,disabled,armed,onSave}: {
 source:unknown;disabled:boolean;armed:boolean;onSave:(brain:string,advisor:boolean,paper:boolean)=>Promise<void>;
}) {
 const view=safeObject(source),available=view.controls_available===true;
 const brains=Array.isArray(view.brains)?view.brains:[];
 const [selectedBrain,setSelectedBrain]=useState<string|null>(null);
 const selected=safeObject(brains.find(b=>String(safeObject(safeObject(b).config).id||'')===selectedBrain));
 const selectedControl=safeObject(selected.control),selectedActivity=safeObject(selected.activity);
 return <section className="operator-card concept-brain-controls" aria-label="Brain controls">
  <h3><ShieldAlert size={18}/> مشاركة العقول في التحليل والتعلّم الافتراضي</h3>
  <p>الأزرار تتحكم في دور العقل الاستشاري وبحوث Paper فقط. لا تسمح لأي عقل ثانوي بإرسال صفقات حقيقية منفرداً.</p>
  {!brains.length&&<p className="safe-note">بانتظار قائمة العقول الفعلية من Core؛ البطاقات الفارغة أدناه لا تمثل حالة أو نتائج.</p>}
  <div className="concept-brain-card-grid">{!brains.length&&Array.from({length:12},(_,i)=><div key={'placeholder-'+i} className="concept-brain-card concept-brain-placeholder" aria-hidden="true"><i/><i/><i/></div>)}{brains.filter((b)=>safeObject(safeObject(b).config).id!=='system').map((b,i)=>{
    const row=safeObject(b),id=String(safeObject(row.config).id||'');
    if(!id)return null;
    const state=safeObject(row.control),activity=safeObject(row.activity),orders=safeObject(row.attributed_orders);
    const integrity=safeObject(row.integrity),evidence=safeObject(integrity.broker_evidence);
    const count=Number(evidence.sample_count),rate=evidence.win_rate;
    const verified=available&&view.account_matches===true&&evidence.source==='closed_costed_mt5_demo_deals_only'&&
      evidence.scope==='current_demo_account_symbol_mode_brain'&&evidence.no_auto_promotion===true&&
      typeof rate==='number'&&Number.isFinite(rate)&&rate>=0&&rate<=1&&Number.isInteger(count)&&count>=30;
    return <article key={`${id}-${i}`} className={'concept-brain-card '+(selectedBrain===id?'selected':'')}>
      <button type="button" className="concept-brain-card-head" onClick={()=>setSelectedBrain(id)} aria-pressed={selectedBrain===id}><ShieldAlert size={24}/><span><b>{String(row.name||id)}</b><small>{id}</small></span><span className={'concept-control-state '+(available&&state.advisor!==false?'on':'')}>{available?(state.advisor===false?'متوقف':'نشط'):'غير موثق'}</span></button>
      <div className="concept-brain-card-metrics"><span>إشارات <b>{available&&view.account_matches===true&&typeof activity.signals==='number'?activity.signals:'—'}</b></span><span>تنفيذ موثق <b>{available&&view.account_matches===true&&typeof orders.fill_acknowledged==='number'?orders.fill_acknowledged:'—'}</b></span><span>نسبة النجاح <b>{verified?`${(rate*100).toFixed(1)}%`:'—'}</b></span></div>
      <div className="concept-brain-card-actions"><button disabled={disabled||armed||!available} onClick={()=>void onSave(id,state.advisor===false,state.paper!==false)}>المشاركة: {available?(state.advisor===false?'إيقاف':'تشغيل'):'—'}</button>
      <button disabled={disabled||armed||!available} onClick={()=>void onSave(id,state.advisor!==false,state.paper===false)}>Paper: {available?(state.paper===false?'إيقاف':'تشغيل'):'—'}</button></div>
    </article>;
  })}</div>
  <div className="concept-brain-inspector"><h3><ShieldAlert size={18}/> تفاصيل العقل المختار</h3>{selectedBrain&&Object.keys(selected).length?<div><strong>{String(selected.name||selectedBrain)}</strong><p>المشاركة: {available?(selectedControl.advisor===false?'متوقفة':'مفعّلة'):'غير مؤكدة'} · Paper: {available?(selectedControl.paper===false?'متوقف':'مفعّل'):'غير مؤكد'}</p><p>فحوص التحليل: {available&&typeof selectedActivity.checks==='number'?selectedActivity.checks:'—'} · الإشارات المرصودة: {available&&typeof selectedActivity.signals==='number'?selectedActivity.signals:'—'}</p><small>تُطبّق التغييرات من أزرار المشاركة وPaper على البطاقة، ضمن الحساب والوضع الحاليين.</small></div>:<p>اختَر بطاقة عقل لعرض حالته الفعلية.</p>}</div>
  <p className="safe-note">يتطلب التعديل Core متصلاً بحساب MT5 مطابقاً، وإيقاف START، وعدم وجود أوامر وسيط غير محسومة.</p>
 </section>;
}

export function ExecutionBindingOperations({source,telemetry,ready,armed,disabled,onBind}: {
 source:unknown;telemetry:{demo:boolean;login:number;server:string;currency:string;symbol:string;account_trade_mode?:number|null}|null;
 ready:boolean;armed:boolean;disabled:boolean;
 onBind:(target:'demo'|'live',deviationPoints:number,confirmation:string)=>Promise<void>;
}){
 const saved=safeObject(safeObject(source).settings);
 const [target,setTarget]=useState<'demo'|'live'>('demo');
 const [deviation,setDeviation]=useState('20');
 const [login,setLogin]=useState('');
 const [error,setError]=useState('');
 const disabledByAccount=!ready||!telemetry||armed||disabled;
 const submit=async(e:FormEvent)=>{
  e.preventDefault();
  const points=Number(deviation);
  if(!Number.isInteger(points)||points<0||points>100000){setError('انزلاق الوسيط المسموح يجب أن يكون بين 0 و100000 نقطة');return}
  if(!telemetry||!ready){setError('لا يوجد اتصال MT5 حديث وموثوق');return}
  if(target==='demo' && (!telemetry.demo||telemetry.account_trade_mode!==0)){
   setError('الحساب الموجود في MT5 ليس حساب Demo مطابقاً');return;
  }
  if(target==='live' && (telemetry.demo||telemetry.account_trade_mode!==2||login.trim()!==String(telemetry.login))){
   setError('حساب REAL يتطلب تسجيل دخول صحيح من MT5 وتأكيد رقمه حرفياً');return;
  }
  setError('');await onBind(target,points,login.trim());setLogin('');
 };
 return <form className="operator-card" aria-label="Broker execution binding" onSubmit={e=>void submit(e)}>
  <h3><ShieldAlert size={18}/> ربط التنفيذ بحساب MT5 الموثوق</h3>
  <p>هذه الخطوة ضرورية قبل START. الربط لا يفتح أي صفقة؛ Core يرفض الحساب غير المطابق ولا يسمح للعقول الثانوية بتنفيذ مستقل.</p>
  <div className="execution-binding-summary">حساب MT5: <b>{telemetry?.login??'—'}</b> · <b>{telemetry?.server||'غير متصل'}</b> · نوع الحساب <b>{telemetry?(telemetry.demo?'Demo':'REAL'):'—'}</b></div>
  <div className="execution-binding-summary">الربط المحفوظ: <b>{String(saved.target||'demo').toUpperCase()}</b> · الحساب <b>{String(saved.login||'—')}</b> · <b>{String(saved.server||'—')}</b></div>
  <div className="operator-grid">
   <label>الحساب الذي سينفذ<select value={target} onChange={e=>{setTarget(e.target.value as 'demo'|'live');setLogin('')}}><option value="demo">MT5 Demo</option><option value="live">MT5 REAL — يتطلب تأكيداً</option></select></label>
   <label>الانحراف المسموح (points)<input type="number" min="0" max="100000" step="1" value={deviation} onChange={e=>setDeviation(e.target.value)} required/></label>
  </div>
  {target==='live'&&<label>تأكيد حساب REAL: اكتب رقم الحساب {telemetry?.login??'—'}<input type="text" inputMode="numeric" autoComplete="off" value={login} onChange={e=>setLogin(e.target.value)} placeholder="اكتب رقم حساب MT5 REAL" required/></label>}
  {error&&<div className="operator-error" role="alert">{error}</div>}
  <div className="quick-ops"><button type="submit" disabled={disabledByAccount}>ربط حساب {target==='demo'?'Demo':'REAL'} باستخدام قواعد Core</button></div>
  {armed&&<p className="safe-note">اضغط وقف الدخول أولاً قبل تبديل حساب التنفيذ أو تغيير الانحراف.</p>}
 </form>;
}
