import {useEffect,useState} from 'react';
import type {FormEvent} from 'react';
import {ShieldAlert,Wallet,SlidersHorizontal} from 'lucide-react';
import {smallAccountPreview,validatePortfolio} from './micro-capital';
import type {MicroRisk,MicroStatus,PortfolioSettings} from './micro-capital';

const record=(x:unknown):Record<string,unknown>=>x!==null&&typeof x==='object'&&!Array.isArray(x)?x as Record<string,unknown>:{};
const shown=(v:number|null,digits=2)=>v===null?'—':v.toLocaleString('en-US',{maximumFractionDigits:digits});

export function MicroCapitalOperations({status,risk,portfolio,armed,disabled,onSavePortfolio}:{
 status:MicroStatus|null;risk:MicroRisk|null;portfolio:unknown;
 armed:boolean;disabled:boolean;onSavePortfolio:(settings:PortfolioSettings)=>Promise<void>;
}){
 const [scenario,setScenario]=useState('500');
 const [draft,setDraft]=useState<PortfolioSettings|null>(null);
 const [error,setError]=useState('');
 const p=record(portfolio),limits=record(p.limits);
 const hasSource=typeof p.enabled==='boolean'&&typeof limits.max_positions==='number'&&
   typeof limits.max_lots==='number'&&typeof limits.max_risk_percent==='number';
 const stored:PortfolioSettings|null=hasSource?{
  enabled:p.enabled===true,max_positions:Number(limits.max_positions),
  max_lots:Number(limits.max_lots),max_risk_percent:Number(limits.max_risk_percent)
 }:null;
 const sourceIdentity=JSON.stringify(stored);
 useEffect(()=>{setDraft(null);setError('')},[sourceIdentity]);
 const edit=draft??stored;
 const patch=(key:keyof PortfolioSettings,value:number|boolean)=>{
  if(!edit)return;
  setDraft({...edit,[key]:value});
 };
 const view=smallAccountPreview(status,risk,Number(scenario));
 const save=async(e:FormEvent)=>{
  e.preventDefault();
  try{
   if(!edit||!hasSource)throw Error('بيانات المحفظة من Core غير متاحة');
   const next=validatePortfolio(edit);
   if(next.enabled&&next.max_risk_percent>2&&!window.confirm(
     'حد المحفظة الجديد أعلى من 2% من Equity. هذا قد يسبب خسارة كبيرة على حساب صغير. هل تريد حفظه؟'))return;
   setError('');await onSavePortfolio(next);
  }catch(e){setError(String(e instanceof Error?e.message:e))}
 };
 return <section className="operator-card" aria-label="Micro capital and portfolio control">
  <h3><Wallet size={18}/> التحكم بالحساب الصغير — Micro REAL / DEMO</h3>
  <p>ماكو حد إيداع يفرضه هذا القسم: ممكن تبدأ بحساب 50 أو 100 من عملة الوسيط إذا شروط العقد والهامش تسمح. فحص Core وMT5 النهائي يبقى إلزامياً.</p>
  <div className="execution-binding-summary">
   نوع MT5: <b>{view.accountType}</b> · Equity: <b>{shown(view.equity)} {view.currency}</b> · الرافعة الحقيقية: <b>{view.leverage===null?'غير متاحة':`1:${view.leverage}`}</b>
  </div>
  <div className="operator-grid">
   <label>رافعة للمقارنة فقط — لا تغيّر الوسيط
    <input aria-label="indicative scenario leverage" type="number" min="1" max="10000" step="1"
      value={scenario} onChange={e=>setScenario(e.target.value)}/>
   </label>
   <label>أقل لوت عند الوسيط<strong>{shown(view.minimumLot,4)}</strong></label>
   <label>الهامش الحر الفعلي<strong>{shown(view.freeMargin)} {view.currency}</strong></label>
   <label>هامش أقل لوت من MT5<strong>{shown(view.minimumLotMargin)} {view.currency}</strong></label>
   <label>هامش أقل لوت عند رافعة المقارنة (تقريبي)<strong>{shown(view.scenarioMargin)} {view.currency}</strong></label>
   <label>ميزانية وقف الصفقة من إعداد Core (تقديرية)<strong>{shown(view.budget)} {view.currency} {view.budgetPercent===null?'':`(${view.budgetPercent}%)`}</strong></label>
  </div>
  <p className="safe-note">{view.note}. {view.marginCovered===false?'الهامش الحر لا يغطي الحد الأدنى المبلغ من MT5.':view.marginCovered===true?'الهامش المبدئي قد يكفي، لكن لا يضمن قبول الصفقة.':'لا تتوفر معلومات هامش كافية.'} الرافعة الأعلى لا تقلل خسارة الستوب لنفس اللوت، واحتساب العمولة والانزلاق والحجم الفعلي يتم عند تنفيذ الوسيط.</p>
  <h3><SlidersHorizontal size={17}/> الحد الإجمالي للمحفظة (مستقل عن Risk لكل صفقة)</h3>
  <p>القيم أدناه يقرأها البرنامج مباشرة من Rust Core. إذا فعّلت الحد الإجمالي وكان أقل من مخاطرة Micro REAL، ممكن يرفض دخول جديد. تقدر تتحكم به وأنت بوضع STOP.</p>
  {stored?<form onSubmit={e=>void save(e)}>
   <div className="operator-grid">
    <label className="operator-toggle"><input type="checkbox" checked={edit?.enabled===true}
       disabled={armed||disabled} onChange={e=>patch('enabled',e.target.checked)}/> تطبيق حد المحفظة الاختياري</label>
    <label>أقصى عدد مراكز<input aria-label="portfolio max positions" type="number" min="1" max="100" step="1" required
       disabled={armed||disabled} value={edit?.max_positions??''} onChange={e=>patch('max_positions',Number(e.target.value))}/></label>
    <label>مجموع اللوت<input aria-label="portfolio max lots" type="number" min="0.001" max="10000" step="0.001" required
       disabled={armed||disabled} value={edit?.max_lots??''} onChange={e=>patch('max_lots',Number(e.target.value))}/></label>
    <label>حد المخاطرة الإجمالية % Equity<input aria-label="portfolio max risk percent" type="number" min="0.01" max="100" step="0.01" required
       disabled={armed||disabled} value={edit?.max_risk_percent??''} onChange={e=>patch('max_risk_percent',Number(e.target.value))}/></label>
   </div>
   {error&&<div className="operator-error" role="alert">{error}</div>}
   <div className="quick-ops"><button type="submit" disabled={armed||disabled}>حفظ حدود المحفظة في Core</button>
    <button type="button" disabled={disabled} onClick={()=>{setDraft(null);setError('')}}>إلغاء التعديلات</button></div>
  </form>:<p className="safe-note">بيانات حد المحفظة غير متاحة؛ لن أختلق قيماً قابلة للحفظ.</p>}
  <p className="safe-note"><ShieldAlert size={14}/> حتى مع إلغاء الحد الاختياري تبقى فحوص Stop Loss وحجم العقد المقبول والرصيد والهامش والتأكد من هوية الحساب ومنع تكرار الأوامر فاعلة. تعديل الرافعة الحقيقية يتم عند الوسيط فقط.</p>
 </section>;
}
