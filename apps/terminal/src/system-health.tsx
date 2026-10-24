import {Activity,ArrowUpRight,Clock3,Cpu,DatabaseZap,ShieldCheck,Wifi,Workflow} from 'lucide-react';
import type {PageId,Status} from './types';
import {feedEvidence} from './feed';

type Tone='good'|'warn'|'bad'|'idle'|'preview';
export type HealthRow={key:string;label:string;value:string;detail:string;tone:Tone;page:PageId};
export type HealthSnapshot={tone:Tone;title:string;subtitle:string;rows:HealthRow[];lastSync:string;quoteAge:string};

// Everything in LIVE mode comes from the V4 Core status or confirmed read-only
// Core configuration. A missing sample is never labelled healthy or given a
// made-up uptime, CPU load, execution latency, or health percentage.
export function buildHealth(status:Status|null,online:boolean,preview:boolean,
  riskLoaded:boolean,lastStatusAt:number|null,now:number):HealthSnapshot{
  if(preview)return {
    tone:'preview',title:'DESIGN PREVIEW',subtitle:'عرض التصميم فقط • ليس MT5',
    lastSync:'—',quoteAge:'—',rows:[
      {key:'core',label:'Rust Core',value:'DEMO',detail:'حالة توضيحية',tone:'preview',page:'events'},
      {key:'mt5',label:'MT5 Feed',value:'DEMO',detail:'لا يوجد حساب فعلي',tone:'preview',page:'settings'},
      {key:'quote',label:'Market Data',value:'DEMO',detail:'شموع توضيحية',tone:'preview',page:'analysis'},
      {key:'execution',label:'Execution',value:'LOCKED',detail:'التداول محظور في المعاينة',tone:'idle',page:'execution'},
      {key:'risk',label:'Risk Monitor',value:'DEMO',detail:'مؤشرات توضيحية',tone:'preview',page:'risk'}]
  };
  const evidence=feedEvidence(status,online,lastStatusAt,now);
  const coreCurrent=evidence.core;
  const brokerOk=evidence.broker;
  const quoteOk=evidence.quote;
  const age=evidence.quoteAgeMs;
  const requested=status?.requested_run===true;
  const executionOk=quoteOk&&requested&&status?.execution_enabled===true;
  const title=!coreCurrent?'CORE OFFLINE':!brokerOk?'FEED OFFLINE':!quoteOk?'QUOTE STALE':executionOk?'EXECUTION ACTIVE':'MONITORING';
  const tone:Tone=!coreCurrent?'bad':!brokerOk||!quoteOk?'warn':'good';
  const rows:HealthRow[]=[
    {key:'core',label:'Rust Core',value:coreCurrent?'ONLINE':'OFFLINE',detail:'تحديث قناة الحالة',tone:coreCurrent?'good':'bad',page:'events'},
    {key:'mt5',label:'MT5 Bridge',value:brokerOk?'CONNECTED':'OFFLINE',detail:brokerOk?'اتصال الوسيط موثوق':'التغذية غير متاحة',tone:brokerOk?'good':'bad',page:'settings'},
    {key:'quote',label:'Market Feed',value:quoteOk?'FRESH':'UNAVAILABLE',detail:quoteOk?`آخر سعر: ${Math.round(age as number)} ms`:'سعر قديم أو غير متاح',tone:quoteOk?'good':'warn',page:'analysis'},
    {key:'execution',label:'Execution',value:executionOk?'ARMED':requested?'BLOCKED':'STOPPED',detail:executionOk?'المحرك أجاز التنفيذ':requested?'غير مؤهل للتنفيذ':'لم يُطلب التشغيل',tone:executionOk?'good':requested?'warn':'idle',page:'execution'},
    {key:'risk',label:'Risk Config',value:coreCurrent&&riskLoaded?'LOADED':'UNKNOWN',detail:coreCurrent&&riskLoaded?'إعدادات متاحة • ليست ضمان أمان':'حالة الضوابط غير مؤكدة',tone:'idle',page:'risk'}
  ];
  return {tone,title,subtitle:brokerOk?'V4 CORE • VERIFIED MT5':'No verified market feed',rows,
    lastSync:coreCurrent&&lastStatusAt!==null?new Date(lastStatusAt).toLocaleTimeString('en-GB',{hour12:false}):'—',
    quoteAge:quoteOk?`${Math.round(age as number)} ms`:'—'};
}

export default function SystemHealth({health,onOpen}:{health:HealthSnapshot;onOpen:(page:PageId)=>void}){
 const count=health.rows.filter(x=>x.tone==='good').length;
 return <section className={`health-widget tone-${health.tone}`} aria-label="System Health — صحة النظام" data-testid="system-health">
   <button type="button" className="health-top" onClick={()=>onOpen('events')} title={`صحة النظام: ${health.title} — اضغط لعرض سجل الأحداث`}>
    <div className="health-emblem"><Activity size={22} strokeWidth={2.4}/><span className="health-pulse"/></div>
    <div className="health-heading"><small>SYSTEM HEALTH</small><strong>صحة النظام</strong></div>
    <ArrowUpRight size={15} className="health-expand"/>
   </button>
   <div className="health-state-line"><span className="health-orb"/><div><strong>{health.title}</strong><small>{health.subtitle}</small></div></div>
   <div className="health-signal" aria-hidden="true">{health.rows.map(row=><i key={row.key} className={`signal-${row.tone}`}/>)}</div>
   <div className="health-rows">
     {health.rows.map((row,i)=>{
       const Icon=[Cpu,Wifi,DatabaseZap,Workflow,ShieldCheck][i];
       return <button key={row.key} className="health-row" onClick={()=>onOpen(row.page)} title={`${row.label}: ${row.value} — ${row.detail}`} type="button">
        <Icon size={12}/><span>{row.label}</span><b className={`health-value state-${row.tone}`}>{row.value}</b>
       </button>
     })}
   </div>
   <div className="health-footer"><span><Clock3 size={11}/> {health.lastSync}</span><span>{health.tone==='preview'?'SAMPLE':`${count}/5 SIGNALS`}</span></div>
 </section>;
}
