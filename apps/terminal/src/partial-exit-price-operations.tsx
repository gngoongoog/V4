import {useState} from 'react';
import type {Status} from './types';

const object=(v:unknown):Record<string,unknown>=>
 v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Record<string,unknown>:{};
const good=(v:unknown):v is number=>typeof v==='number'&&Number.isFinite(v)&&v>0;
export type PriceProposal={context:string;positionIdentifier:number;
 planIds:[string,string,string];prices:[number,number,number]};

/** Operator-only saved research prices. No stage authorization or send path. */
export function PartialExitPriceOperations({status,source,disabled,onSave,onConsent}:{
 status:Status|null;source:unknown;disabled:boolean;
 onSave:(proposal:PriceProposal)=>Promise<boolean>;
 onConsent:(context:string,positionIdentifier:number,planIds:[string,string,string],enabled:boolean,confirmLiveLogin:string)=>Promise<boolean>;
}){
 const [selected,setSelected]=useState(''),[values,setValues]=useState<[string,string,string]>(['','','']);
 const [dirty,setDirty]=useState(false);
 const [liveConfirm,setLiveConfirm]=useState('');
 const t=status?.telemetry,view=object(source),settings=object(view.settings);
 const stages=object(view.partial_exit_stage_plans);
 const prices=object(view.partial_exit_price_proposals);
 const consent=object(view.partial_exit_stage_consent);
 const sealed=object(view.partial_exit_stage_sealed_core);
 const context=t?JSON.stringify([t.server,t.login,t.symbol,t.currency]):'';
 const target=settings.target==='live'?'live':settings.target==='demo'?'demo':null;
 const accountModeMatches=!!t&&((target==='demo'&&t.demo===true&&t.account_trade_mode===0)||
  (target==='live'&&t.demo===false&&t.account_trade_mode===2));
 const available=!!t&&status?.gateway_fresh===true&&view.account_matches===true&&
  view.context===context&&t.connected&&accountModeMatches&&
  t.margin_mode===2&&settings.login===t.login&&
  settings.server===t.server&&settings.currency===t.currency&&
  stages.schema==='partial-exit-stage-plans-v1'&&stages.context===context&&
  stages.target===target&&stages.read_only===true&&stages.orders_enabled===false&&
  stages.stage_execution_supported===false&&
  prices.schema==='partial-exit-prices-v1'&&prices.context===context&&
  prices.target===target&&prices.read_only===true&&prices.orders_enabled===false;
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
 const consentValid=available&&consent.schema==='tp-stage-operator-consent-v1'&&
  consent.status==='account_scoped_operator_consent_not_execution'&&
  consent.purpose==='preparation_only'&&
  consent.context===context&&consent.target===target&&consent.read_only===true&&
  consent.automatic_tp_execution_enabled===false&&consent.orders_created===0;
 const consentRows=consentValid&&Array.isArray(consent.rows)?consent.rows.map(object):[];
 const approval=consentRows.find(r=>r.position_identifier===Number(position?.identifier));
 const sealedScoped=available&&sealed.schema==='tp-stage-sealed-core-intents-v1'&&
  sealed.status==='account_scoped_prepared_not_sent'&&
  sealed.context===context&&sealed.target===target&&
  sealed.read_only===true&&sealed.orders_enabled===false&&
  sealed.broker_commands_created===0;
 const prepared=sealedScoped&&Array.isArray(sealed.rows)?sealed.rows.map(object):[];
 const sealedTp1=prepared.find(r=>r.stage===1&&
  r.position_identifier===Number(position?.identifier)&&
  r.physical_ticket===position?.ticket&&
  Array.isArray(planIds)&&r.plan_id===planIds[0]&&
  r.broker_command_id===null&&r.gateway_send_id===null&&
  r.broker_order_ticket===null);
 const approvalMatches=!!approval&&approval.position_ticket===position?.ticket&&
  Array.isArray(approval.plan_ids)&&!!planIds&&
  JSON.stringify(approval.plan_ids)===JSON.stringify(planIds)&&
  approval.read_only===true&&approval.purpose==='preparation_only'&&
  approval.orders_enabled===false&&
  approval.broker_command_id===null;
 const approved=approvalMatches&&approval.operator_approved===true&&
  approval.state==='operator_approved_not_dispatched';
 // Historical operator consent is not permission to send. A binding revision
 // or immutable price/position change must offer revocation, not reapproval.
 const staleConsent=approvalMatches&&
  ['operator_approval_revision_mismatch','operator_approval_stale_needs_review']
   .includes(String(approval?.state));
 const revokeOther=consentRows.filter(r=>
  (r.operator_approved===true||
   ['operator_approval_revision_mismatch','operator_approval_stale_needs_review']
    .includes(String(r.state)))&&
  (r.position_identifier!==Number(position?.identifier)||!approvalMatches));
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
 const consentLocked=disabled||!available||status?.requested_run===true||!consentValid;
 const submitConsent=(next:boolean)=>{
  if(consentLocked||!position||!planIds||!recordedValid||
   (next&&target==='live'&&liveConfirm!==String(t?.login))||
   (next&&staleConsent)||(approval&&!approvalMatches))return;
  void onConsent(context,Number(position.identifier),planIds,next,liveConfirm).then(ok=>{
   if(ok)setLiveConfirm('');
  });
 };
 const submit=()=>{
  if(locked||!valid||!position||!planIds||!dirty)return;
  void onSave({context,positionIdentifier:Number(position.identifier),planIds,
   prices:targets as [number,number,number]}).then(ok=>{
    if(ok){setDirty(false);setValues(['','',''])}
  });
 };
 return <section className="operator-card" aria-label="تسعير أهداف TP للحساب المختار">
  <h3>أسعار أهداف TP1 / TP2 / TP3 — {target==='live'?'حساب Live':'حساب Demo'}</h3>
  <p className="safe-note">هنا تحفظ أسعار ثلاثة أهداف بحثية للمركز الحالي. لا تُرسل أوامر، ولا تغيّر TP الموجود عند الوسيط، ولا تُفعّل التعادل أو المرحلة التالية. الحفظ نهائي لهذه المسودة ولا يمكن تعديل الأسعار بصمت.</p>
  {!available?<p>اربط الحساب الذي اخترته (Demo أو Live) بنظام Hedging، وتأكد من تطابق بيانات Core وMT5 وحداثتها قبل حفظ الأسعار.</p>:
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
    {position&&sealedTp1?<div className="execution-binding-summary" aria-label="سجل هوية TP1 داخل Core">
     <p className="safe-note">سجل هوية TP1 داخل Core — لا يوجد أمر مرسل إلى MT5 أو تفويض لتنفيذ الإغلاق.</p>
     <p>{sealedTp1.state==='sealed_unarmed_not_dispatched'?
       'تم ختم هوية TP1 للمركز الحالي، وهي غير مفعّلة للإرسال.':
       sealedTp1.state==='historical_seal_expired_no_send'?
       'سجل TP1 تاريخي، تغيرت البيانات أو انتهت صلاحية السعر؛ لا يمكن إرساله.':
       'سجل TP1 غير موثوق ويحتاج مراجعة؛ لا يمكن إرساله.'}</p>
    </div>:null}
    {position&&recorded&&recordedValid&&consentValid?<div className="execution-binding-summary" aria-label="موافقة مشغل TP المرحلية">
     <p className="safe-note">موافقة المشغل مرتبطة بهذا المركز فقط، لكنها حالياً مرحلة تحضير وليست تفويضاً بالوسيط. تشغيل TP التلقائي ما زال مقفلاً في Core؛ لا يوجد إرسال أو تعديل على MT5 من هذا الزر.</p>
     <p>{approved?'موافقة بحثية مُثبتة للحساب المحدد — لا أوامر':
      staleConsent?'الموافقة المحفوظة قديمة أو تغيّر المركز/إعدادات التنفيذ؛ يجب إلغاء الموافقة القديمة قبل مراجعة الخطة. لا يُسمح بإعادة تفعيلها بصمت.':
       'لم تُثبت موافقة تنفيذ هذه الخطة بعد'}</p>
     {target==='live'&&!approved&&!staleConsent?<label>اكتب رقم حساب Live للتحقق من الموافقة
      <input type="text" autoComplete="off" value={liveConfirm}
       aria-label="تأكيد رقم حساب Live لخطة TP" disabled={consentLocked}
       onChange={e=>setLiveConfirm(e.target.value)} placeholder={String(t?.login||'')}/>
     </label>:null}
     <button type="button" disabled={consentLocked||(approval&&!approvalMatches)||
       (!approved&&!staleConsent&&target==='live'&&liveConfirm!==String(t?.login))}
       onClick={()=>submitConsent(!(approved||staleConsent))}>
       {staleConsent?'إلغاء الموافقة القديمة غير الصالحة':
        approved?'إلغاء الموافقة المحفوظة':'تسجيل موافقة هذه الخطة للتحضير المستقبلي (لا إرسال)'}
     </button>
    </div>:null}
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
     <button type="submit" disabled={locked||!valid||!dirty}>حفظ أسعار الأهداف لحساب {target==='live'?'Live':'Demo'} (معاينة بدون إرسال)</button>
     {!valid&&dirty?<p className="safe-note">الأهداف لازم تكون على شبكة Tick وفي اتجاه الربح وبالترتيب، وخارج نطاق السعر الحالي. يُراجع Core أيضاً هوية المركز ومسودة TP وTP الوسيط الأصلي.</p>:null}
    </>:null}
   </form>}
  {consentValid&&revokeOther.length>0?<div className="execution-binding-summary" aria-label="إلغاء موافقات TP القديمة">
   <p className="safe-note">موافقات مخزونة من مراكز ثانية أو تغيّر وضعها. ماكو تنفيذ تلقائي منها؛ تكدر تلغيها بعد STOP حتى لو المركز ما عاد متاح بالقائمة.</p>
   {revokeOther.map((r,i)=>{
    const ids=r.plan_ids;
    const validIds=Array.isArray(ids)&&ids.length===3&&
     ids.every((v:unknown)=>typeof v==='string'&&/^tp-plan-[a-f0-9]{32}$/.test(v));
    const verified=r.purpose==='preparation_only'&&r.read_only===true&&
     r.orders_enabled===false&&r.broker_command_id===null&&
     Number.isSafeInteger(r.position_identifier)&&Number(r.position_identifier)>0&&validIds;
    return <div className="brain-control-row" key={String(r.position_identifier)+'-'+i}>
     <span>مركز {String(r.position_identifier)} · تحضير فقط</span>
     <button type="button" disabled={!verified||consentLocked}
      onClick={()=>{if(verified)void onConsent(context,Number(r.position_identifier),
       ids as [string,string,string],false,'')}}>
      إلغاء موافقة المركز {String(r.position_identifier)}
     </button>
    </div>;
   })}
  </div>:null}
 </section>;
}
