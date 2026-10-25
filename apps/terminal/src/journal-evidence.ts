import type {Status} from './types';

export type BrokerJournalRow={
 positionId:number;symbol:string;side:'BUY'|'SELL';status:string;
 opened:string;lotsOpen:number|null;lotsClosed:number|null;remaining:number|null;
 profit:number|null;commission:number|null;swap:number|null;fee:number|null;net:number|null;
 provenCosts:boolean;brain:string;reason:string;
};
export type BrokerJournalView={ready:boolean;reason:string;total:number|null;offset:number;limit:number;hasPrev:boolean;hasNext:boolean;rows:BrokerJournalRow[]};
const obj=(v:unknown):Record<string,unknown>=>v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Record<string,unknown>:{};
const validInt=(v:unknown):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=0;
const good=(v:unknown):v is number=>typeof v==='number'&&Number.isFinite(v);
const numberOrNull=(v:unknown):number|null=>good(v)?v:null;
const states=new Set(['CLOSED','OPEN','PARTIAL / OPEN','EXIT SEEN / CLOSE NOT VERIFIED','AWAITING CLOSE HISTORY','OPEN / AWAITING DEAL SYNC']);
const no=(reason:string,offset:number,limit:number):BrokerJournalView=>({
 ready:false,reason,total:null,offset,limit,hasPrev:false,hasNext:false,rows:[]
});
export function journalEvidence(status:Status|null,execution:unknown,history:unknown,offset:number,limit:number):BrokerJournalView{
 const t=status?.telemetry,bound=obj(execution),settings=obj(bound.settings),v=obj(history);
 if(!validInt(offset)||offset>1000000||!validInt(limit)||limit<1||limit>100)
  return no('حدود الصفحة غير صالحة',0,25);
 if(!status?.gateway_fresh||!t?.connected||!Number.isSafeInteger(t.login)||t.login<=0||
   !t.server||!t.currency||!t.symbol||
   !((t.demo===true&&t.account_trade_mode===0)||(t.demo===false&&t.account_trade_mode===2))||
   bound.account_matches!==true||settings.target!==(t.demo?'demo':'live')||
   settings.login!==t.login||settings.server!==t.server||settings.currency!==t.currency||
   bound.context!==JSON.stringify([t.server,t.login,t.symbol,t.currency]))
  return no('يجب اتصال نفس حساب MT5 المربوط بالتنفيذ لعرض سجل الصفقات، ويمكنه استئناف التصفح بعد العودة',offset,limit);
 if(v.offset!==offset||v.limit!==limit||!validInt(v.total)||
   !Array.isArray(v.rows)||v.rows.length>500)
  return no('استجابة Core لا تطابق الصفحة المطلوبة أو غير مكتملة',offset,limit);
 const rows:BrokerJournalRow[]=[];
 for(const item of v.rows){
  const p=obj(item),origin=obj(p.origin);
  if(!validInt(p.position_id)||p.position_id===0||p.symbol!==t.symbol||
      (p.side!=='BUY'&&p.side!=='SELL')||!states.has(String(p.status)))
   return no('سجل Core لا يطابق الرمز أو إثبات المركز',offset,limit);
  const verified=p.booked_cost_evidence==='BROKER_COMPONENTS_VERIFIED'&&
   ['broker_net_booked','broker_gross','commission','swap','fee'].every(k=>good(p[k]));
  rows.push({
   positionId:p.position_id,symbol:String(p.symbol),side:p.side as 'BUY'|'SELL',
   status:String(p.status),opened:typeof p.opened_utc==='string'?p.opened_utc.slice(0,45):'—',
   lotsOpen:numberOrNull(p.lots_opened),lotsClosed:numberOrNull(p.lots_closed),
   remaining:numberOrNull(p.remaining_lots),provenCosts:verified,
   net:verified?numberOrNull(p.broker_net_booked):null,
   profit:verified?numberOrNull(p.broker_gross):null,
   commission:verified?numberOrNull(p.commission):null,
   swap:verified?numberOrNull(p.swap):null,fee:verified?numberOrNull(p.fee):null,
   brain:typeof origin.opening_brain==='string'?origin.opening_brain.slice(0,80):'غير موثق',
   reason:typeof origin.reason==='string'?origin.reason.slice(0,350):'لا يتوفر سبب فتح موثق'
  });
 }
 return {ready:true,reason:'إيصالات MT5 المحفوظة من Core، لا تشمل Paper/Shadow',
  total:v.total,offset,limit,hasPrev:offset>0,
  hasNext:offset+limit<v.total,rows};
}
