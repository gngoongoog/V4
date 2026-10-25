import type {Status} from './types';
import type {BrokerJournalView} from './journal-evidence';

/** Broker-verified closed-position sample only, not an account statement,
 * projection, or a backtest. Rates require a sufficiently sized cohort.
 */
export type BrokerReport={
 ready:boolean;reason:string;sampled:number;fullyClosed:number;
 unverified:number;allPositions:number|null;complete:boolean;mode:'DEMO'|'REAL'|null;
 wins:number|null;losses:number|null;winRate:number|null;
 net:number|null;gross:number|null;commission:number|null;swap:number|null;fee:number|null;
};
const blank=(reason:string):BrokerReport=>({ready:false,reason,sampled:0,fullyClosed:0,
 unverified:0,allPositions:null,complete:false,mode:null,wins:null,losses:null,
 winRate:null,net:null,gross:null,commission:null,swap:null,fee:null});
export function brokerReport(status:Status|null,source:BrokerJournalView|null):BrokerReport{
 if(!source?.ready||!status?.gateway_fresh||!status.telemetry?.connected)
  return blank(source?.reason||'لا يوجد سجل صفقات MT5 مثبت للحساب الحالي');
 const t=status.telemetry;
 if(t.demo!==true&&t.demo!==false)return blank('نوع الحساب غير مؤكد');
 // A verified journal already enforces exact Core binding + account context.
 // All values are restricted to fully closed, booked broker receipts.
 const verified=source.rows.filter(r=>r.status==='CLOSED'&&r.provenCosts&&
  r.net!==null&&r.profit!==null&&r.commission!==null&&r.swap!==null&&r.fee!==null);
 const unverified=source.rows.length-verified.length;
 const count=verified.length;
 let net=0,gross=0,commission=0,swap=0,fee=0,wins=0,losses=0;
 for(const row of verified){
  net+=row.net!;gross+=row.profit!;commission+=row.commission!;
  swap+=row.swap!;fee+=row.fee!;
  if(row.net!>0)wins++;
  else if(row.net!<0)losses++;
 }
 const finite=[net,gross,commission,swap,fee].every(Number.isFinite);
 if(!finite)return blank('مجموع الرسوم غير صالح؛ لا تعرض تقديراً بديلاً');
 // Latest 100 positions only: an incomplete sample MUST not masquerade
 // as the complete account statement or lifetime performance.
 const allPositions=source.total;
 return {ready:true,reason:count?
   'عينة من مراكز MT5 المغلقة المسجلة بعد التكاليف':'لا توجد مراكز مغلقة مكتملة الرسوم في هذه العينة',
  sampled:source.rows.length,fullyClosed:count,unverified,
  allPositions,complete:source.offset===0&&allPositions!==null&&
    allPositions<=source.limit&&source.rows.length===allPositions,
  mode:t.demo?'DEMO':'REAL',wins:count?wins:null,losses:count?losses:null,
  // A win rate based on 1 or 2 MT5 trades is not representative. Do not
  // display a headline success rate until at least 30 real closed examples.
  winRate:count>=30?100*wins/count:null,
  net:count?net:null,gross:count?gross:null,commission:count?commission:null,
  swap:count?swap:null,fee:count?fee:null};
}
