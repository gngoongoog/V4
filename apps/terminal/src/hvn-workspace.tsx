import {useEffect,useState} from 'react';
import {requestHvnResearch,readPanel} from './bridge';
import type {Status} from './types';
import './hvn-workspace.css';

type Obj=Record<string,unknown>;
const obj=(v:unknown):Obj=>v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Obj:{};
const rows=(v:unknown):Obj[]=>Array.isArray(v)?v.map(obj).slice(0,8):[];
const value=(v:unknown)=>typeof v==='number'&&Number.isFinite(v)?v:null;
const format=(v:unknown,places=3)=>value(v)===null?'—':Number(v).toLocaleString('en-US',{maximumFractionDigits:places});
const showDate=(v:unknown)=>value(v)!==null&&Number(v)>0?new Date(Number(v)).toLocaleString('ar-IQ'):'غير مسجل';
export function HvnWorkspace({status,connected,preview,disabled}:{
 status:Status|null;connected:boolean;preview:boolean;disabled:boolean;
}){
 const [data,setData]=useState<Obj|null>(null);
 const [windowMs,setWindowMs]=useState(600_000);
 const [loading,setLoading]=useState(false);
 const [error,setError]=useState('');
 const [notice,setNotice]=useState('');
 const report=obj(data?.report);
 const pending=data?.pending===true;
 const account=status?.telemetry;
 const allowed=!preview&&!disabled&&connected&&status?.requested_run===false&&
    account?.positions?.length===0&&account?.pending_orders===0;
 useEffect(()=>{
  if(preview)return;
  let active=true;
  const refresh=()=>{void readPanel('/v1/hvn').then(r=>{if(active)setData(obj(r))})
    .catch(e=>{if(active)setError('تعذر قراءة بحث HVN من Core: '+String(e))})};
  refresh();
  const interval=setInterval(refresh,2000);
  return()=>{active=false;clearInterval(interval)};
 },[preview]);
 const start=async()=>{
  if(!allowed||loading||pending)return;
  setLoading(true);setError('');setNotice('');
  try{
   const result=obj(await requestHvnResearch(windowMs));
   if(result.queued!==true)throw new Error('Core لم يؤكد إدراج طلب HVN');
   setNotice('تم طلب قياس تاريخ تيكات الوسيط؛ انتظر رد Core. ليس إشارة دخول.');
   setData(obj(await readPanel('/v1/hvn')));
  }catch(e){setError(String(e))}
  finally{setLoading(false)}
 };
 const source=report.source==='mt5_broker_trade_volume_real'?
  'حجم صفقات Last الذي أبلغه هذا الوسيط — ليس حجم COMEX':
  report.source==='mt5_quote_tick_activity'?
  'عدد تغيّرات أسعار Bid/Ask عند منتصف السعر — ليس حجم صفقات':
  'لا يوجد مصدر حجم متحقق';
 const resultAvailable=report.available===true && !preview;
 const nodes=resultAvailable?rows(report.nodes):[];
 const headerIdentity=obj(report.identity);
 const contextVerified=connected&&String(headerIdentity.login??'')===String(account?.login??'')&&
  headerIdentity.server===account?.server&&headerIdentity.symbol===account?.symbol&&
  headerIdentity.currency===account?.currency;
 return <section className="hvn-workspace" aria-label="HVN — عقد النشاط المرتفع" dir="rtl">
  <div className="hvn-head">
   <div><small>RESEARCH ONLY · NO ORDERS</small><h3>عقل HVN البحثي — عقد النشاط المرتفع</h3>
   <p>يستخرج مستويات تركّز النشاط من تيكات MT5 المرصودة، وليس من توزيع حجم الشمعة بالتساوي. لا يعتمد للتداول ولا يعطي نسبة نجاح.</p></div>
   <div className="hvn-controls"><label>نافذة القياس
    <select aria-label="نافذة HVN" value={windowMs} disabled={loading||pending} onChange={e=>setWindowMs(Number(e.target.value))}>
     <option value={60000}>دقيقة</option><option value={300000}>5 دقائق</option>
     <option value={600000}>10 دقائق</option><option value={900000}>15 دقيقة</option>
    </select></label>
    <button type="button" onClick={()=>void start()} disabled={!allowed||loading||pending}>
     {loading?'جارٍ إرسال الطلب':pending?'القياس جارٍ':'قياس HVN من تيكات MT5'}
    </button></div>
  </div>
  {preview?<p className="hvn-warning">معاينة تصميم فقط — لا نتائج HVN حقيقية ولا عمليات وسيط.</p>:
   <p className="hvn-caption">{!connected?'MT5 غير مؤكد؛ لا بيانات حساب أو نتيجة بحث قابلة للعرض':
    status?.requested_run?'أوقف الدخول STOP لإتاحة قراءة البحث؛ لن نقاطع القناص أثناء التداول':
    account?.positions?.length||account?.pending_orders!==0?'يجب أن يخلو الحساب من المراكز والأوامر أولاً':
    pending?'طلب البحث قيد التنفيذ لدى Gateway':'البحث منفصل تماماً عن تنفيذ الصفقات وإدارة العقول الحالية'}</p>}
  {error&&<p role="alert" className="hvn-error">{error}</p>}
  {notice&&<p role="status" className="hvn-caption">{notice}</p>}
  {!preview&&typeof data?.note==='string'&&data.note.length>0&&<p className="hvn-caption">{data.note}</p>}
  {!preview&&report.available===false&&connected&&
   <p className="hvn-warning">لا توجد عقد صالحة: {String(report.reason||'البيانات غير كافية')}</p>}
  {resultAvailable&&contextVerified?
   <><div className="hvn-metrics">
    <div><span>نوع البيانات</span><strong>{source}</strong></div>
    <div><span>أحداث مرصودة</span><strong>{format(report.observations,0)}</strong></div>
    <div><span>مستويات سعرية</span><strong>{format(report.distinct_levels,0)}</strong></div>
    <div><span>آخر Tick مستخدم</span><strong>{showDate(report.observed_to_ms)}</strong></div>
   </div>
   <div className="hvn-grid"><div className="hvn-grid-head"><b>مستويات HVN المرصودة</b><small>مرتبة حسب النشاط؛ أدلة وصفية لا احتمالات دخول</small></div>
    <div className="hvn-scroll"><table><thead><tr><th>السعر</th><th>النشاط</th><th>الحصة من العينة</th></tr></thead>
    <tbody>{nodes.map((r,i)=><tr key={i}><td>{format(r.price,8)}</td><td>{format(r.activity,4)}</td><td>{format(value(r.share)===null?null:Number(r.share)*100,2)}%</td></tr>)}</tbody>
    </table></div></div>
   <p className="hvn-caption">الحساب {String(headerIdentity.login||'—')} · {String(headerIdentity.server||'—')} · {String(headerIdentity.symbol||'—')} · المصدر: {String(report.source)}. {String(report.note||'')}</p>
   </>:
   <p className="hvn-empty">{preview?'لا تعرض المعاينة نتائج تداول':!connected?'بانتظار حساب MT5 حديث':pending?'استلام سجل Tick وتحليل العقد جارٍ':'لم تصل بعد نتيجة HVN متوافقة مع الحساب الحالي'}</p>}
  <p className="hvn-warning">للدقة: XAUUSD لدى وسيط CFD لا يكشف بالضرورة حجم عقود الذهب المركزية CME/COMEX. هذا القسم لا يفتح صفقات، ولا يعدّل عقول التنفيذ أو المخاطر.</p>
 </section>;
}
