import {useState} from 'react';
import type {Status} from './types';

const object=(v:unknown):Record<string,unknown>=>
 v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Record<string,unknown>:{};
const good=(v:unknown):v is number=>typeof v==='number'&&Number.isFinite(v)&&v>0;
export type PriceProposal={context:string;positionIdentifier:number;
 planIds:[string,string,string];prices:[number,number,number]};

/** Operator-only saved research prices. No stage authorization or send path. */
export function PartialExitPriceOperations({status,source,disabled,onSave}:{
 status:Status|null;source:unknown;disabled:boolean;
 onSave:(proposal:PriceProposal)=>Promise<boolean>;
}){
 const [selected,setSelected]=useState(''),[values,setValues]=useState<[string,string,string]>(['','','']);
 const [dirty,setDirty]=useState(false);
 const t=status?.telemetry,view=object(source),settings=object(view.settings);
 const stages=object(view.partial_exit_stage_plans);
 const prices=object(view.partial_exit_price_proposals);
 const context=t?JSON.stringify([t.server,t.login,t.symbol,t.currency]):'';
 const available=!!t&&status?.gateway_fresh===true&&view.account_matches===true&&
  view.context===context&&t.connected&&t.demo===true&&t.account_trade_mode===0&&
  t.margin_mode===2&&settings.target==='demo'&&settings.login===t.login&&
  settings.server===t.server&&settings.currency===t.currency&&
  stages.schema==='partial-exit-stage-plans-v1'&&stages.context===context&&
  stages.target==='demo'&&stages.read_only===true&&stages.orders_enabled===false&&
  stages.stage_execution_supported===false&&
  prices.schema==='partial-exit-prices-v1'&&prices.context===context&&
  prices.target==='demo'&&prices.read_only===true&&prices.orders_enabled===false;
 const stageRows=available&&Array.isArray(stages.rows)?stages.rows.map(object):[];
 const stored=available&&Array.isArray(prices.rows)?prices.rows.map(object):[];
 const positions=t?.positions||[];
 const eligible=positions.filter(p=>Number.isSafeInteger(p.identifier)&&Number(p.identifier)>0&&p.ticket>0&&
  ['BUY','SELL'].includes(p.side)&&stageRows.filter(r=>r.position_identifier===Number(p.identifier)
    &&r.state==='prepared_only').length===3);
 const position=eligible.find(p=>String(p.identifier)===selected);
 const plans=position?stageRows.filter(r=>r.position_identifier===Number(position.identifier))
  .sort((a,b)=>Number(a.stage)-Number(b.stage)):[];
 const planIds=plans.length===3&&plans.every((r,i)=>r.stage===i+1&&
  typeof r.plan_id==='string'&&/^tp-plan-[a-f0-9]{32}$/.test(r.plan_id as string))?
  plans.map(r=>String(r.plan_id)) as [string,string,string]:null;
 const recorded=stored.find(r=>r.position_identifier===Number(position?.identifier));
 const recordedValid=!!recorded&&
  ['price_levels_saved_no_execution','quote_stale_no_trigger'].includes(String(recorded.state))&&
  Array.isArray(recorded.prices)&&recorded.prices.length===3&&
  recorded.prices.every(good);
 const targets=values.map(Number);
 const tick=Number(t?.tick_size);
 const valid=values.every(v=>v.trim()!=='')&&targets.every(good)&&
  good(tick)&&targets.every(v=>Math.abs(v/tick-Math.round(v/tick))<=1e-6)&&
  !!position&&!!planIds&&good(position.price_open)&&
  (position.side==='BUY'?
    targets[0]>Math.max(position.price_open,Number(t?.ask))+tick*.5&&
    targets[1]>targets[0]+tick*.5&&targets[2]>targets[1]+tick*.5:
    targets[0]+tick*.5<Math.min(position.price_open,Number(t?.bid))&&
    targets[1]+tick*.5<targets[0]&&targets[2]+tick*.5<targets[1]);
 const locked=disabled||!available||status?.requested_run===true||!!recorded;
 const submit=()=>{
  if(locked||!valid||!position||!planIds||!dirty)return;
  void onSave({context,positionIdentifier:Number(position.identifier),planIds,
   prices:targets as [number,number,number]}).then(ok=>{
    if(ok){setDirty(false);setValues(['','',''])}
  });
 };
 return <section className="operator-card" aria-label="تسعير أهداف TP التجريبية">
  <h3>أسعار أهداف TP1 / TP2 / TP3 — DEMO فقط</h3>
  <p className="safe-note">هنا تحفظ أسعار ثلاثة أهداف بحثية للمركز الحالي. لا تُرسل أوامر، ولا تغيّر TP الموجود عند الوسيط، ولا تُفعّل التعادل أو المرحلة التالية. الحفظ نهائي لهذه المسودة ولا يمكن تعديل الأسعار بصمت.</p>
  {!available?<p>هذه الخاصية تحتاج حساب Demo بنظام Hedging، وبيانات Core وMT5 حديثة ومطابقة.</p>:
   <form onSubmit={e=>{e.preventDefault();submit()}}>
    <label>المركز المطلوب تسعيره
     <select aria-label="اختيار مركز لتسعير TP" value={selected} disabled={disabled||status?.requested_run===true}
      onChange={e=>{setSelected(e.target.value);setValues(['','','']);setDirty(false)}}>
      <option value="">اختر مركزاً بثلاث مراحل محفوظة</option>
      {eligible.map(p=><option key={p.identifier} value={String(p.identifier)}>
       {p.symbol} · {p.side} · تذكرة {p.ticket} · {p.volume} لوت
      </option>)}
     </select>
    </label>
    {eligible.length===0?<p>لا توجد مراكز مؤهلة بأسعار متاحة وثلاث خطط حجم سليمة. قد تكون الخطط بحاجة إلى مراجعة بعد تغيّر المركز.</p>:null}
    {position&&recorded?<p className="safe-note">
      {recordedValid?<>
       محفوظ للمركز {position.ticket}: {(recorded.prices as number[]).join(' / ')}.
       هذه أسعار توثيقية فقط وليست أهدافاً مفعّلة لدى MT5.
      </>:<>
       بيانات خطة TP غير موثوقة وتحتاج مراجعة: لا يمكن اعتماد الأسعار القديمة
       أو إعادة حفظها تلقائياً. احتفظ بسجل الوسيط الأصلي للمراجعة، ولا تعتبر
       وصول السعر إلى أي هدف دليلاً على تنفيذ صفقة.
      </>}
    </p>:null}
    {position&&!recorded?<>
     <p className="safe-note">دخول: {position.price_open} · Bid: {t?.bid??'—'} · Ask: {t?.ask??'—'} · Tick: {t?.tick_size??'—'} · الاتجاه: {position.side}</p>
     <div className="protection-cards">
      {values.map((value,i)=><label key={i}>
       سعر TP{i+1} {i===0?'(50%)':i===1?'(30%)':'(20%)'}
       <input type="number" aria-label={'سعر TP'+(i+1)} min="0" step="any"
        value={value} disabled={locked} placeholder="مثال: 2001.00"
        onChange={e=>{const next:[string,string,string]=[...values];next[i]=e.target.value;
         setValues(next);setDirty(true)}}/>
      </label>)}
     </div>
     <button type="submit" disabled={locked||!valid||!dirty}>حفظ أسعار البحث لـ Demo فقط</button>
     {!valid&&dirty?<p className="safe-note">الأهداف لازم تكون على شبكة Tick وفي اتجاه الربح وبالترتيب، وخارج نطاق السعر الحالي. يُراجع Core أيضاً هوية المركز ومسودة TP وTP الوسيط الأصلي.</p>:null}
    </>:null}
   </form>}
 </section>;
}
