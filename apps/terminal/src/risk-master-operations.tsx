import {ShieldCheck} from 'lucide-react';
import {useState} from 'react';
import type {Status} from './types';
import type {RiskRecovery} from './bridge';

const object=(value:unknown):Record<string,unknown>=>value&&typeof value==='object'&&!Array.isArray(value)?value as Record<string,unknown>:{};

export function RiskMasterOperations({status,source,disabled,armed,onSet}:{
 status:Status|null;source:unknown;disabled:boolean;armed:boolean;onSet:(enabled:boolean,recovery:RiskRecovery|null)=>Promise<void>;
}){
 const [automatic,setAutomatic]=useState(true);
 const [basis,setBasis]=useState<'balance'|'equity'>('balance');
 const [threshold,setThreshold]=useState('500');
 const view=object(source),risk=object(view.risk_master),settings=object(view.settings),t=status?.telemetry;
 const context=t?JSON.stringify([t.server,t.login,t.symbol,t.currency]):'';
 const available=!!t&&status?.gateway_fresh===true&&t.connected&&view.account_matches===true&&view.context===context&&
  settings.login===t.login&&settings.server===t.server&&settings.currency===t.currency&&
  settings.target===(t.demo?'demo':'live')&&t.account_trade_mode===(t.demo?0:2)&&
  risk.scope==='bound_broker_account_and_mode'&&typeof risk.enabled==='boolean';
 const enabled=available?risk.enabled===true:null;
 const n=Number(threshold),current=t?(basis==='balance'?t.balance:t.equity):NaN;
 const validLimit=threshold.trim()!==''&&Number.isFinite(n)&&n>=0.01&&n<=1e12&&Number.isFinite(current)&&current>0&&n>current;
 const saved=object(risk.recovery);
 return <section className="risk-master-card" aria-label="مفتاح إدارة المخاطر للحساب">
  <div><ShieldCheck size={20}/><strong>إدارة المخاطر للحساب</strong><span className={enabled===true?'teal':enabled===false?'red':''}>{enabled===null?'غير مؤكدة':enabled?'مفعّلة':'متوقفة'}</span></div>
  <p>عند الإيقاف تتوقف ميزانية وقف الصفقة، والحدود الاختيارية، وحدود المحفظة وقاطع الخسارة اليومي للمداخل الجديدة. تبقى شروط الوسيط والهوية والهامش وSL/TP الحالية وتسوية الأوامر. هذا الإعداد خاص بالحساب ونوعه.</p>
  {enabled===true&&<div className="operator-grid">
   <label className="operator-toggle"><input type="checkbox" checked={automatic} disabled={disabled||armed} onChange={e=>setAutomatic(e.target.checked)}/> إعادة التفعيل تلقائياً عند الحد</label>
   <label>نوع الحد<select aria-label="نوع حد استعادة المخاطر" value={basis} disabled={disabled||armed||!automatic} onChange={e=>setBasis(e.target.value as 'balance'|'equity')}><option value="balance">الرصيد Balance</option><option value="equity">حقوق الملكية Equity</option></select></label>
   <label>حد الاستعادة ({t?.currency||'عملة الحساب'})<input aria-label="حد استعادة المخاطر" type="number" min="0.01" max="1000000000000" step="any" value={threshold} disabled={disabled||armed||!automatic} onChange={e=>setThreshold(e.target.value)}/></label>
  </div>}
  {enabled===true&&automatic&&!validLimit&&<small role="alert">اختر حداً موجباً أعلى من قيمة الحساب الحالية.</small>}
  {enabled===false&&typeof saved.threshold==='number'&&<small>الاستعادة التلقائية عند {saved.basis==='equity'?'Equity':'الرصيد'} {saved.threshold} {t?.currency}. بعد الوصول تبقى المخاطر مفعّلة حتى لو انخفض الحساب.</small>}
  <button type="button" disabled={disabled||armed||!available||(enabled===true&&automatic&&!validLimit)} onClick={()=>void onSet(!enabled,enabled&&automatic?{basis,threshold:n}:null)}>
   {enabled===false?'تفعيل إدارة المخاطر':'إيقاف إدارة المخاطر'}
  </button><small>يتطلب تغيير المفتاح يدوياً STOP وتسوية الأوامر. الاستعادة التلقائية تعتمد على تحديث MT5 وتُفحص أيضاً قبل إرسال الدخول. Micro REAL له حد مستقل.</small>
 </section>;
}
