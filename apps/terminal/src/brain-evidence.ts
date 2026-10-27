// P1-B: dashboard KPIs are derived ONLY from Core's account-scoped, read-only
// P1 evidence contract. No paper, cached other-account, stale, REAL or
// below-threshold Demo sample is permitted to become a displayed win rate.
export type Evidence={samples:number;wins:number;losses:number;breakeven:number;winRate:number;net:number;scope:string};
export function brokerEvidence(report:unknown,status:unknown):Evidence|null{
 if(!report||typeof report!=='object'||!status||typeof status!=='object')return null;
 const r=report as Record<string,any>;
 const s=status as Record<string,any>;
 const t=s.telemetry;
 if(s.gateway_fresh!==true||!t||t.connected!==true||t.demo!==true||
    t.account_trade_mode!==0||!Number.isSafeInteger(t.login)||t.login<=0||
    typeof t.server!=='string'||!t.server||
    typeof t.symbol!=='string'||!t.symbol||
    typeof t.currency!=='string'||!t.currency)return null;
 const expectedContext=JSON.stringify([t.server,t.login,t.symbol,t.currency]);
 if(r.context!==expectedContext||r.account_matches!==true||r.settings?.target!=='demo')return null;
 const gate=r.brain_integrity;
 if(gate?.schema!=='p1-brain-integrity-v1'||gate?.inventory_complete!==true||
    gate?.brains_accounted_for!==17||gate?.demo_account_verified!==true||
    gate?.account_trade_mode!==0||gate?.read_only!==true||
    gate?.changes_execution!==false||gate?.automatic_promotion!==false||
    !['sniper','scalping','swing','expert'].includes(gate?.trading_mode)||
    gate.trading_mode!==r.trading_mode)return null;
 if(!Array.isArray(r.brains)||r.brains.length!==17)return null;
 const system=r.brains.filter((b:any)=>b?.config?.id==='system');
 if(system.length!==1)return null;
 const entry=system[0];
 const integrity=entry.integrity;
 const evidence=integrity?.broker_evidence;
 const audit=entry.quality;
 const summary=audit?.all;
 if(integrity?.role!=='selected_mode_coordinator'||integrity?.promotion_enabled!==false||
    evidence?.source!=='closed_costed_mt5_demo_deals_only'||
    evidence?.scope!=='current_demo_account_symbol_mode_brain'||
    evidence?.no_auto_promotion!==true||evidence?.broker_execution_permission!==false||
    audit?.schema!=='closed-demo-learning-quality-v1'||
    audit?.learning_eligible_broker_samples_only!==true||audit?.no_execution_change!==true||
    audit?.policy!=='read_only'||audit?.paper_and_forward_excluded!==true||
    audit?.no_auto_promotion!==true)return null;
 // A mixed source/policy revision must be reviewed separately, not collapsed
 // into a headline KPI. Chronological monitoring is not forward OOS evidence.
 if(!['monitor_only','recent_deterioration'].includes(evidence?.status)||
    evidence.status!==audit?.status)return null;
 const n=evidence.sample_count,wins=summary?.wins_after_costs;
 const losses=summary?.losses_after_costs,breakeven=summary?.breakeven_after_costs;
 const rate=evidence.win_rate,net=evidence.net_booked;
 const threshold=evidence.minimum_samples;
 if(!Number.isInteger(threshold)||threshold<30||
    !Number.isInteger(n)||n<threshold||n!==audit?.broker_samples||
    n!==summary?.samples||
    !Number.isInteger(wins)||wins<0||wins>n||
    !Number.isInteger(losses)||losses<0||!Number.isInteger(breakeven)||breakeven<0||
    wins+losses+breakeven!==n||
    typeof rate!=='number'||!Number.isFinite(rate)||rate<0||rate>1||
    typeof summary.win_rate!=='number'||!Number.isFinite(summary.win_rate)||
    Math.abs(rate-wins/n)>1e-9||Math.abs(rate-summary.win_rate)>1e-9||
    typeof net!=='number'||!Number.isFinite(net)||
    typeof summary.net_booked!=='number'||!Number.isFinite(summary.net_booked)||
    Math.abs(net-summary.net_booked)>1e-6)return null;
 return {samples:n,wins,losses,breakeven,winRate:rate*100,net,
   scope:'MT5 DEMO • Coordinator • Mode / Account / Symbol • Online-monitor only'};
}
