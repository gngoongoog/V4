// V25 P0-B: broker journal quality evidence. The UI can display it, not trade from it.
export type QualityStatus={
 gateway_fresh?:boolean;
 telemetry?:{connected?:boolean;demo?:boolean;account_trade_mode?:number|null;
 login?:number;server?:string;symbol?:string;currency?:string}|null;
};
export type QualityObservation={
 ready:boolean; reason:string;
 sampleSize:number|null;rejects:number|null;unknown:number|null;unresolved:number|null;
 categoryCounts:Record<string,number>;
 circuit:{state:string;policy:string;reason:string;unknown:number;rejections:number}|null;
 queueP95:number|null;sendP95:number|null;totalP95:number|null;
 rows:Array<{brain:string;kind:string;state:string;createdMs:number;
   category:string;retcode:number|null}>;
};
const none=(reason:string):QualityObservation=>({
 ready:false,reason,sampleSize:null,rejects:null,unknown:null,unresolved:null,
 categoryCounts:{},circuit:null,queueP95:null,sendP95:null,totalP95:null,rows:[]
});
const obj=(v:unknown):Record<string,unknown>=>v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Record<string,unknown>:{};
const positiveInt=(v:unknown):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>0;
const nonNegativeInt=(v:unknown):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=0;
const measured=(v:unknown):number|null=>typeof v==='number'&&Number.isFinite(v)&&v>=0&&v<=60000?v:null;
const accountOK=(a:Record<string,unknown>,t:NonNullable<QualityStatus['telemetry']>)=>
 a.login===t.login&&a.server===t.server&&a.symbol===t.symbol&&
 a.currency===t.currency&&a.mode===t.account_trade_mode;
const states=new Set(['queued','dispatched','unknown','placed','partial','filled',
 'rejected','cancelled','expired','observed','closed']);
export function qualityObservation(status:QualityStatus|null,source:unknown,nowMs:number):QualityObservation{
 const t=status?.telemetry,q=obj(source),a=obj(q.account);
 if(!status?.gateway_fresh||!t?.connected||!positiveInt(t.login)||!t.server||
    !t.symbol||!t.currency||
    !((t.demo===true&&t.account_trade_mode===0)||
      (t.demo===false&&t.account_trade_mode===2))||
    q.schema!=='p1f-execution-quality-v1'||q.read_only!==true||
    q.account_verified!==true||q.state!=='observed'||
    !Number.isSafeInteger(nowMs)||!positiveInt(q.observed_ms)||
    q.observed_ms>nowMs+2000||nowMs-q.observed_ms>15000||
    !accountOK(a,t)||q.context!==JSON.stringify([t.server,t.login,t.symbol,t.currency])||
    !nonNegativeInt(q.sample_size)||q.sample_size>100||
    !nonNegativeInt(q.rejected_count)||q.rejected_count>q.sample_size||
    !nonNegativeInt(q.unknown_count)||q.unknown_count>q.sample_size||
    !nonNegativeInt(q.unresolved_count)||q.unresolved_count>q.sample_size||
    !Array.isArray(q.rows)||q.rows.length>30
 )return none('لا يوجد تقرير تنفيذ حديث ومطابق للحساب الحالي');
 const categories:Record<string,number>={};
 for(const [key,value] of Object.entries(obj(q.rejection_categories))){
   if(!/^[a-z_]{1,48}$/.test(key)||!nonNegativeInt(value)||value>q.rejected_count)
     return none('فئات الرفض غير صالحة');
   categories[key]=value;
 }
 if(Object.values(categories).reduce((a,b)=>a+b,0)!==q.rejected_count)
    return none('عدد الرفض لا يطابق الأسباب المسجلة');
 const rows:QualityObservation['rows']=[];
 for(const item of q.rows){
  const r=obj(item);
  if(typeof r.brain!=='string'||typeof r.kind!=='string'||
     typeof r.state!=='string'||!states.has(r.state)||
     !nonNegativeInt(r.created_ms)||typeof r.category!=='string'||
     !/^[a-z_]{1,64}$/.test(r.category)||
     !(r.retcode===null||Number.isSafeInteger(r.retcode)))return none('نتائج الأوامر غير صالحة');
  rows.push({brain:r.brain.slice(0,48),kind:r.kind.slice(0,12),state:r.state,
   createdMs:r.created_ms,category:r.category,retcode:typeof r.retcode==='number'?r.retcode:null});
 }
 const rawCircuit=obj(q.entry_circuit);
 const circuit=rawCircuit.schema==='p0c-core-quality-v1'&&rawCircuit.read_only===true&&
   ['ready','blocked','demo_advisory','unavailable'].includes(String(rawCircuit.state))&&
   ['required_real','advisory_demo'].includes(String(rawCircuit.policy))&&
   typeof rawCircuit.reason==='string'&&/^[a-z_]{1,80}$/.test(rawCircuit.reason)&&
   typeof rawCircuit.entry_allowed==='boolean'&&
   nonNegativeInt(rawCircuit.unknown_open_intents)&&
   nonNegativeInt(rawCircuit.recent_broker_rejections)&&
   rawCircuit.observed_ms===q.observed_ms&&
   rawCircuit.never_blocks_close_or_reconcile===true&&
   (t.demo?rawCircuit.policy==='advisory_demo':rawCircuit.policy==='required_real')?
    {state:String(rawCircuit.state),policy:String(rawCircuit.policy),
     reason:String(rawCircuit.reason),unknown:Number(rawCircuit.unknown_open_intents),
     rejections:Number(rawCircuit.recent_broker_rejections)}:null;
 const qMetric=(key:string)=>{const m=obj(q[key]);return nonNegativeInt(m.samples)&&m.samples>0?
  measured(m.p95_ms):null};
 return {ready:true,reason:'قراءة من سجل أوامر Core، وليس تاريخ أرباح أو تعبئات مضمونة',
   sampleSize:q.sample_size,rejects:q.rejected_count,unknown:q.unknown_count,
   unresolved:q.unresolved_count,categoryCounts:categories,circuit,
   queueP95:qMetric('queue_to_send'),sendP95:qMetric('send'),totalP95:qMetric('total'),
   rows};
}
