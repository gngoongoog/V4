import type {BrokerJournalRow} from './journal-evidence';

export type ReportPoint={at:number;label:string;net:number;total:number};
export function reportSeries(rows:BrokerJournalRow[]):ReportPoint[]{
 const ordered=rows.flatMap(row=>{
  if(row.status!=='CLOSED'||!row.provenCosts||row.net===null||!Number.isFinite(row.net))return [];
  const at=Date.parse(row.latestDeal);
  return Number.isFinite(at)?[{at,net:row.net}]:[];
 }).sort((a,b)=>a.at-b.at);
 let total=0;const result:ReportPoint[]=[];
 for(const item of ordered){
  total+=item.net;if(!Number.isFinite(total))return [];
  result.push({at:item.at,label:new Date(item.at).toISOString().slice(0,10),net:item.net,total});
 }
 return result;
}
