// Closed DEMO outcomes credited to the system coordinator, for the currently
// bound account / symbol / trading mode. NEVER paper trades, open positions,
// a cross-account account-wide win rate, or a guarantee of profitability.
export type Evidence={samples:number;wins:number;losses:number;winRate:number;net:number;scope:string};
export function brokerEvidence(report:unknown):Evidence|null{
 if(!report||typeof report!=='object')return null;
 const brains=(report as {brains?:unknown}).brains;
 if(!Array.isArray(brains))return null;
 const system=brains.find(b=>b?.config?.id==='system');
 const audit=system?.quality;const summary=audit?.all;
 if(audit?.source!=='Closed MT5 DEMO deal records; net=gross+commission+swap+fee'||audit?.paper_and_forward_excluded!==true)return null;
 const n=summary?.samples,wins=summary?.wins_after_costs,rate=summary?.win_rate,net=summary?.net_booked;
 if(!Number.isInteger(n)||n<=0||!Number.isInteger(wins)||wins<0||wins>n||
    typeof rate!=='number'||!Number.isFinite(rate)||rate<0||rate>1||
    Math.abs(rate-wins/n)>0.00001||typeof net!=='number'||!Number.isFinite(net))return null;
 return {samples:n,wins,losses:n-wins,winRate:rate*100,net,
   scope:'MT5 DEMO • Coordinator • Mode / Account / Symbol'};
}
