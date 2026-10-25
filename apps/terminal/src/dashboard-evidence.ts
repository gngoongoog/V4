import {qualityObservation} from './execution-quality';
import type {QualityObservation} from './execution-quality';
import type {Status} from './types';

export type DashboardRead={scope:string;readMs:number;execution:unknown;trades:unknown;quality:unknown};
export type DashboardTrade={positionId:number;symbol:string;side:string;status:string;lots:number|null;net:number|null};
export type DashboardView={ready:boolean;reason:string;trades:DashboardTrade[];quality:QualityObservation|null;total:number|null};
const obj=(x:unknown):Record<string,unknown>=>x!==null&&typeof x==='object'&&!Array.isArray(x)?x as Record<string,unknown>:{};
const good=(x:unknown):x is number=>typeof x==='number'&&Number.isFinite(x);
const nonneg=(x:unknown):x is number=>typeof x==='number'&&Number.isSafeInteger(x)&&x>=0;
const statuses=new Set(['CLOSED','OPEN','PARTIAL / OPEN','EXIT SEEN / CLOSE NOT VERIFIED','AWAITING CLOSE HISTORY','OPEN / AWAITING DEAL SYNC']);
const empty=(reason:string):DashboardView=>({ready:false,reason,trades:[],quality:null,total:null});

/** G3: read-only triple Core report gated by matching bound MT5 account and
 * scope. The dashboard must never infer fills, fees, or PnL from sample data.
 */
export function dashboardView(status:Status|null,read:DashboardRead|null,scope:string,now:number):DashboardView{
 const t=status?.telemetry;
 const execution=obj(read?.execution);
 const binding=obj(execution.settings);
 if(!read||read.scope!==scope||!Number.isSafeInteger(read.readMs)||
    read.readMs>now+2000||now-read.readMs>20000||
    !status?.gateway_fresh||!t?.connected||
    !Number.isSafeInteger(t.login)||t.login<=0||!t.server||!t.symbol||!t.currency||
    !((t.demo===true&&t.account_trade_mode===0)||(t.demo===false&&t.account_trade_mode===2))||
    execution.account_matches!==true||
    // MT5 account type is not part of journal context's four-field key;
    // verify the bound Demo/REAL target plus the actual persisted identity,
    // even if a stale or incorrectly cached account_matches says true.
    binding.target!==(t.demo?'demo':'live')||
    binding.login!==t.login||binding.server!==t.server||
    binding.currency!==t.currency||
    execution.context!==JSON.stringify([t.server,t.login,t.symbol,t.currency]))
  return empty('بانتظار تقرير حديث ومطابق لحساب MT5 المربوط بالتنفيذ');
 const journal=obj(read.trades);
 if(!nonneg(journal.total)||journal.offset!==0||!nonneg(journal.limit)||
    journal.limit>100||!Array.isArray(journal.rows)||journal.rows.length>100)
  return empty('سجل الصفقات من Core غير متاح أو غير مكتمل');
 const rows:DashboardTrade[]=[];
 for(const item of journal.rows.slice(0,5)){
  const trade=obj(item);
  if(!Number.isSafeInteger(trade.position_id)||Number(trade.position_id)<=0||
     trade.symbol!==t.symbol||!['BUY','SELL'].includes(String(trade.side))||
     !statuses.has(String(trade.status)))
   return empty('تقرير الصفقات لا يطابق الرمز أو بيانات الوسيط');
  const net=trade.booked_cost_evidence==='BROKER_COMPONENTS_VERIFIED'&&
    good(trade.broker_net_booked)?trade.broker_net_booked:null;
  const lots=good(trade.lots_opened)&&trade.lots_opened>0?trade.lots_opened:null;
  rows.push({positionId:Number(trade.position_id),symbol:String(trade.symbol),
   side:String(trade.side),status:String(trade.status),lots,net});
 }
 const quality=qualityObservation(status,read.quality,now);
 return {ready:true,reason:'سجل Core المربوط بحساب MT5 الحالي',
   trades:rows,total:journal.total,quality:quality.ready?quality:null};
}
