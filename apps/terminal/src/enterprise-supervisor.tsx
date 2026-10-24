import {useEffect,useRef,useState} from 'react';
import {readPanel} from './bridge';
import './enterprise-supervisor.css';

type Config = {enforce_entry_gate:boolean;max_quote_age_ms:number;max_spread_atr:number;min_closed_bars:number};
const defaults:Config={enforce_entry_gate:false,max_quote_age_ms:2500,max_spread_atr:0.3,min_closed_bars:20};
type Assessment = {entry_allowed?:boolean;guards?:string[];market_regime?:string;recommended_mode?:string|null;observed_spread_to_atr?:number|null;m1_efficiency?:number|null;m5_efficiency?:number|null};
const known=(a:unknown):Record<string,unknown>=>a&&typeof a==='object'&&!Array.isArray(a)?a as Record<string,unknown>:{};
const LABELS:Record<string,string>={sniper:'القناص',scalping:'السكالبر',swing:'السوينك',expert:'الخبير'};
export default function EnterpriseSupervisor({preview,armed,onSave}:{
 preview:boolean;armed:boolean;
 onSave:(settings:Record<string,unknown>)=>Promise<Record<string,unknown>>;
}){
 const [settings,setSettings]=useState<Config>(defaults);
 const initialized=useRef(false);
 const [assessment,setAssessment]=useState<Assessment|null>(null);
 const [activeMode,setActiveMode]=useState('—');
 const [loaded,setLoaded]=useState(false);
 const [enforced,setEnforced]=useState(false);
 const [integrity,setIntegrity]=useState<'unknown'|'ok'|'invalid_fail_closed'>('unknown');
 const [busy,setBusy]=useState(false);
 const [error,setError]=useState('');
 const [notice,setNotice]=useState('');
 useEffect(()=>{
  if(preview){setLoaded(false);setAssessment(null);setEnforced(false);setIntegrity('unknown');initialized.current=false;return;}
  let alive=true;
  const fetchStatus=async()=>{
   try{
    const data=known(await readPanel('/v1/enterprise/supervisor'));
    if(!alive)return;
    const conf=known(data.settings);
    const evidence=known(data.assessment);
    if(!initialized.current){setSettings({...defaults,...conf} as Config);initialized.current=true;}
    setAssessment(evidence as Assessment);
    setEnforced(data.entry_gate_enforced===true);
    setIntegrity(data.configuration_integrity==='ok'?'ok':'invalid_fail_closed');
    setActiveMode(String(data.active_execution_mode||'—'));
    setLoaded(true);setError('');
   }catch(e){if(alive){setError(String(e));setAssessment(null);setLoaded(false);setIntegrity('unknown');initialized.current=false;}}
  };
  void fetchStatus();const timer=setInterval(()=>void fetchStatus(),5000);
  return()=>{alive=false;clearInterval(timer)};
  // Intentionally ignore `loaded`: policy edits should not be overwritten by 5s polling.
 },[preview]);
 const persist=async()=>{
  setBusy(true);setNotice('');setError('');
  try{
   // No direct write is allowed from a child panel: the parent serializes ALL
   // Tauri mutations and confirms their readback for the same account scope.
   const observed=await onSave(settings);
   setEnforced(observed.entry_gate_enforced===true);
   setIntegrity(observed.configuration_integrity==='ok'?'ok':'invalid_fail_closed');
   setNotice('تأكد حفظ السياسة لدى Rust Core عبر قراءة تحقق لنفس جلسة MT5.');
  }
  catch(e){setError(String(e));}finally{setBusy(false);}
 };
 const evidenceProblem=assessment?.entry_allowed===false;
 const blocked=enforced && evidenceProblem;
 const statusUnverified=preview||!loaded||integrity!=='ok';
 const title=preview?'PREVIEW / غير متصل':!loaded?'حالة Core غير معروفة':integrity!=='ok'?'خلل سياسة المنسّق — الدخول محجوب':blocked?'الدخول الجديد محجوب':evidenceProblem?'تحذير غير مُفَعَّل':'أدلة الإدخال مستوفاة';
 return <section className="v20-supervisor" aria-label="المنسق المؤسسي V20">
  <div className="v20-supervisor-heading"><div><small>V20 • ENTERPRISE SUPERVISOR</small><h3>المنسّق المؤسسي للأوضاع الأربعة</h3></div><strong className={statusUnverified||blocked?'v20-blocked':'v20-valid'}>{title}</strong></div>
  <p>توصية تحليلية من بيانات MT5 وليست احتمال نجاح أو أمراً بالتداول. <b>المالك الوحيد للتنفيذ يبقى الوضع الذي اخترته.</b></p>
  <div role="status" className={enforced||!loaded?'v20-enforcement-on':'v20-enforcement-off'}>بوابة الدخول المحفوظة في Core: <strong>{!loaded?'غير معروفة — الاتصال أو القراءة فشلت':integrity!=='ok'?'فساد إعدادات — منع الدخول الجديد قائم':enforced?'مُفَعَّلة — تمنع الدخول غير الموثوق':'غير مُفَعَّلة — التحذيرات استشارية فقط'}</strong></div>
  <div className="v20-stats">
   <div><small>الوضع الحالي</small><b>{LABELS[activeMode]||activeMode}</b></div>
   <div><small>الوضع المقترح</small><b>{assessment?.recommended_mode?LABELS[assessment.recommended_mode]||assessment.recommended_mode:'لا توجد أفضلية واضحة'}</b></div>
   <div><small>حالة السوق</small><b>{assessment?.market_regime||'—'}</b></div>
   <div><small>Spread / ATR</small><b>{assessment?.observed_spread_to_atr == null?'—':assessment.observed_spread_to_atr.toFixed(3)}</b></div>
  </div>
  {evidenceProblem&&!enforced?<p className="v20-error">تنبيه: هذه المخالفات لا تمنع التنفيذ لأنك لم تحفظ تفعيل البوابة لدى Core.</p>:null}
  {integrity==='invalid_fail_closed'?<p className="v20-error" role="alert">إعدادات المنسّق المخزّنة غير صالحة؛ راجع السجلات وأصلحها فقط في وضع STOP ومن دون مراكز أو أوامر معلقة.</p>:null}
  {assessment?.guards?.length?<ul className="v20-guards">{assessment.guards.map((gate,index)=><li key={index}>{gate}</li>)}</ul>:<div className="v20-ok">{loaded && assessment?'لا توجد موانع فنية ضمن الفحوص الحالية. الربحية غير مضمونة.':'لا تتوفر أدلة حديثة موثوقة من Core'}</div>}
  <label className="v20-toggle"><input type="checkbox" checked={settings.enforce_entry_gate} onChange={e=>setSettings({...settings,enforce_entry_gate:e.target.checked})}/><span>تفعيل فحص جودة البيانات قبل أي دخول جديد (اختياري)</span></label>
  <div className="v20-inputs">
   <label>سقف عمر السعر (ms)<input type="number" min="250" max="3000" step="50" value={settings.max_quote_age_ms} onChange={e=>setSettings({...settings,max_quote_age_ms:Number(e.target.value)})}/></label>
   <label>سقف Spread/ATR<input type="number" min="0.01" max="1" step="0.01" value={settings.max_spread_atr} onChange={e=>setSettings({...settings,max_spread_atr:Number(e.target.value)})}/></label>
   <label>أقل عدد شموع<input type="number" min="15" max="100" step="1" value={settings.min_closed_bars} onChange={e=>setSettings({...settings,min_closed_bars:Number(e.target.value)})}/></label>
  </div>
  <div className="v20-actions"><button type="button" disabled={preview||armed||busy||!loaded} onClick={()=>void persist()}>{busy?'جارٍ التحقق...':'حفظ سياسة المنسّق لدى Core'}</button><small>يتطلب STOP واتصال MT5 حديثاً بحساب مطابق وغياب مراكز وأوامر معلقة. يتضمن مسار القناص السريع Fast Pulse.</small></div>
  {error&&<p className="v20-error" role="alert">{error}</p>}{notice&&<p className="v20-ok" role="status">{notice}</p>}
 </section>;
}
