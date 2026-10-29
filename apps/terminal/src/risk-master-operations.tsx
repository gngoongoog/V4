import {ShieldCheck} from 'lucide-react';
import type {Status} from './types';

const object=(value:unknown):Record<string,unknown>=>value&&typeof value==='object'&&!Array.isArray(value)?value as Record<string,unknown>:{};

export function RiskMasterOperations({status,source,disabled,armed,onSet}:{
 status:Status|null;source:unknown;disabled:boolean;armed:boolean;onSet:(enabled:boolean)=>Promise<void>;
}){
 const view=object(source),risk=object(view.risk_master),settings=object(view.settings),t=status?.telemetry;
 const context=t?JSON.stringify([t.server,t.login,t.symbol,t.currency]):'';
 const available=!!t&&status?.gateway_fresh===true&&t.connected&&view.account_matches===true&&view.context===context&&
  settings.login===t.login&&settings.server===t.server&&settings.currency===t.currency&&
  settings.target===(t.demo?'demo':'live')&&t.account_trade_mode===(t.demo?0:2)&&
  risk.scope==='bound_broker_account_and_mode'&&typeof risk.enabled==='boolean';
 const enabled=available?risk.enabled===true:null;
 return <section className="risk-master-card" aria-label="مفتاح إدارة المخاطر للحساب">
  <div><ShieldCheck size={20}/><strong>إدارة المخاطر للحساب</strong><span className={enabled===true?'teal':enabled===false?'red':''}>{enabled===null?'غير مؤكدة':enabled?'مفعّلة':'متوقفة'}</span></div>
  <p>عند الإيقاف تتوقف ميزانية وقف الصفقة، والحدود الاختيارية، وحدود المحفظة وقاطع الخسارة اليومي للمداخل الجديدة. تبقى شروط الوسيط والهوية والهامش وSL/TP الحالية وتسوية الأوامر. هذا الإعداد خاص بالحساب ونوعه.</p>
  <button type="button" disabled={disabled||armed||!available} onClick={()=>void onSet(!enabled)}>
   {enabled===false?'تفعيل إدارة المخاطر':'إيقاف إدارة المخاطر'}
  </button><small>يتطلب STOP وانتظار تسوية الأوامر. الإيقاف يبقى حتى تعيد تفعيله؛ إعداد Micro REAL المنفصل له حد استعادة خاص به.</small>
 </section>;
}
