import {useEffect,useState} from 'react';
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

export function TradingSettingsOperations({source,disabled,armed,apply}: {
 source:unknown;disabled:boolean;armed:boolean;apply:(input:Record<string,unknown>)=>Promise<void>;
}) {
 const view=safeObject(source);
 const stored=safeObject(safeObject(view.config).trading);
 const commission=safeObject(view.commission_editor);
 const [draft,setDraft]=useState<Record<string,unknown>>({});
 const [fee,setFee]=useState('7');const [basis,setBasis]=useState('round_trip');const [error,setError]=useState('');
 const [editedFee,setEditedFee]=useState(false);const [editedBasis,setEditedBasis]=useState(false);
 const sourceId=JSON.stringify([stored,commission.amount,commission.basis]);
 useEffect(()=>{setDraft({});setEditedFee(false);setEditedBasis(false)},[sourceId]);
 const edit=Object.keys(draft).length?draft:stored;
 const patch=(key:string,value:unknown)=>setDraft(old=>({...stored,...old,[key]:value}));
 const mode=String(edit.mode||'expert'),priority=edit.priority_enabled===true;
 const fallback=String(edit.fallback_mode||'expert');
 const sniper=edit.sniper_active===true;
 const save=async()=>{
  const updated={...stored,...draft};
  if(updated.priority_enabled===true){updated.mode=updated.sniper_active===true?'sniper':String(updated.fallback_mode||'expert');}
  if(updated.fallback_mode==='sniper'){setError('الوضع البديل لا يمكن أن يكون قناصاً');return}
  const rate=Number(editedFee?fee:commission.amount??7);
  if(!Number.isFinite(rate)||rate<0||rate>1000){setError('العمولة يجب أن تكون بين 0 و1000 لكل لوت');return}
  setError('');
  await apply({trading:updated,commission_per_lot:rate,commission_basis:editedBasis?basis:String(commission.basis||'round_trip'),slippage_ticks:Number(safeObject(safeObject(view.config).risk).slippage_ticks??2)});
  setDraft({});setEditedFee(false);setEditedBasis(false);
 };
 // The editor retains the user's typing; a new Core revision is not silently
 // applied on top of unsaved fields. Clicking reset re-reads the current scope.
 const reset=()=>{setDraft({});setEditedFee(false);setEditedBasis(false)};
 return <section className="operator-card" aria-label="Coordinator and costs">
  <h3><ShieldAlert size={18}/> منسق التداول وتكاليف الوسيط</h3>
  <p>يوجد منسق واحد فقط لإرسال صفقات MT5. العقول الأخرى تحليلية. لا يمكن تعديل هذه الإعدادات أثناء START.</p>
  <div className="operator-grid">
   <label>وضع تنفيذ الصفقات<select value={mode} disabled={priority||armed} onChange={e=>patch('mode',e.target.value)}>{MODES.map(([v,label])=><option key={v} value={v}>{label}</option>)}</select></label>
   <label>الوضع البديل للقناص<select value={fallback} disabled={armed} onChange={e=>patch('fallback_mode',e.target.value)}>{MODES.filter(([v])=>v!=='sniper').map(([v,label])=><option key={v} value={v}>{label}</option>)}</select></label>
   <label className="operator-toggle"><input type="checkbox" checked={priority} disabled={armed} onChange={e=>{setDraft(old=>({...stored,...old,priority_enabled:e.target.checked,mode:e.target.checked?(sniper?'sniper':fallback):mode}))}}/> تفعيل أولوية القناص</label>
   <label className="operator-toggle"><input type="checkbox" checked={sniper} disabled={!priority||armed} onChange={e=>setDraft(old=>({...stored,...old,sniper_active:e.target.checked,mode:e.target.checked?'sniper':fallback}))}/> القناص هو المنسق النشط</label>
   <label>أسلوب القناص<select value={String(edit.sniper_style||'auto')} disabled={armed} onChange={e=>patch('sniper_style',e.target.value)}>{STYLES.map(s=><option key={s} value={s}>{s}</option>)}</select></label>
   <label>عقل القناص<select value={String(edit.sniper_brain||'classic')} disabled={armed} onChange={e=>patch('sniper_brain',e.target.value)}><option value="classic">Classic</option><option value="learner">Learner</option></select></label>
   <label>العمولة لكل 1 لوت<input aria-label="commission per lot" type="number" min="0" step="0.01" value={editedFee?fee:String(commission.amount??7)} onChange={e=>{setEditedFee(true);setFee(e.target.value)}}/></label>
   <label>طريقة احتساب العمولة<select aria-label="commission basis" value={editedBasis?basis:String(commission.basis||'round_trip')} onChange={e=>{setEditedBasis(true);setBasis(e.target.value)}}><option value="round_trip">دخول وخروج معاً</option><option value="per_side">لكل جهة (مرتين)</option></select></label>
  </div>
  <p className="safe-note">إذا كانت العمولة 7 دولار لكل لوت ذهاباً وإياباً: حجم 0.01 = $0.07، و0.1 = $0.70، و1.0 = $7.00. هذا تقدير تكلفة، والخصم الفعلي من شركة الوساطة.</p>
  {error&&<div role="alert" className="operator-error">{error}</div>}
  <div className="quick-ops"><button disabled={disabled||armed||!Object.keys(stored).length} onClick={()=>void save()}>حفظ الوضع والعمولة لدى Core</button><button disabled={disabled} onClick={reset}>التراجع عن التعديلات</button></div>
  <small>إصدار المصدر: {sourceId.length?'إعدادات Core':'غير متاح'}</small>
 </section>;
}

export function BrainControlsOperations({source,disabled,armed,onSave}: {
 source:unknown;disabled:boolean;armed:boolean;onSave:(brain:string,advisor:boolean,paper:boolean)=>Promise<void>;
}) {
 const view=safeObject(source),available=view.controls_available===true;
 const brains=Array.isArray(view.brains)?view.brains:[];
 return <section className="operator-card" aria-label="Brain controls">
  <h3><ShieldAlert size={18}/> مشاركة العقول في التحليل والتعلّم الافتراضي</h3>
  <p>الأزرار تتحكم في دور العقل الاستشاري وبحوث Paper فقط. لا تسمح لأي عقل ثانوي بإرسال صفقات حقيقية منفرداً.</p>
  {brains.filter((b)=>safeObject(safeObject(b).config).id!=='system').map((b,i)=>{
    const row=safeObject(b),id=String(safeObject(row.config).id||'');
    if(!id)return null;
    const state=safeObject(row.control);
    return <div key={`${id}-${i}`} className="brain-control-row">
      <b>{String(row.name||id)}</b><span>Advisor: {state.advisor===false?'OFF':'ON'} · Paper: {state.paper===false?'OFF':'ON'}</span>
      <button disabled={disabled||armed||!available} onClick={()=>void onSave(id,state.advisor===false,state.paper!==false)}>Toggle Advisor</button>
      <button disabled={disabled||armed||!available} onClick={()=>void onSave(id,state.advisor!==false,state.paper===false)}>Toggle Paper</button>
    </div>;
  })}
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
