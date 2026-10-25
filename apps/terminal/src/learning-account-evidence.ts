import type {Status} from './types';

export type LearningAccountView={
 ready:boolean;reason:string;mode:'DEMO'|'REAL'|null;
 snapshots:number|null;pending:number|null;eligible:number|null;
 incompatible:number|null;lastBar:number|null;
 trainingEnabled:false;predictionEnabled:false;
};
const unavailable=(reason:string):LearningAccountView=>({
 ready:false,reason,mode:null,snapshots:null,pending:null,eligible:null,
 incompatible:null,lastBar:null,trainingEnabled:false,predictionEnabled:false
});
const obj=(x:unknown):Record<string,unknown>=>x!==null&&typeof x==='object'&&!Array.isArray(x)?x as Record<string,unknown>:{};
const count=(x:unknown):x is number=>typeof x==='number'&&Number.isSafeInteger(x)&&x>=0;
export function learningAccountView(status:Status|null,report:unknown,now:number):LearningAccountView{
 const t=status?.telemetry,r=obj(report),a=obj(r.account);
 if(!status?.gateway_fresh||!t?.connected||!Number.isSafeInteger(t.login)||t.login<=0||
   !t.server||!t.currency||!t.symbol||
   !((t.demo===true&&t.account_trade_mode===0)||(t.demo===false&&t.account_trade_mode===2))||
   !Number.isSafeInteger(now)||!Number.isSafeInteger(r.observed_ms)||
   typeof r.observed_ms!=='number'||r.observed_ms<=0||
   r.observed_ms>now+2000||now-r.observed_ms>15000||
   r.schema!=='learning-account-evidence-v1'||r.state!=='observed'||
   r.read_only!==true||r.account_verified!==true||
   r.training_enabled!==false||r.prediction_enabled!==false||
   r.automatic_promotion!==false||r.broker_win_rate!==null||
   r.broker_net_pnl!==null||
   a.server!==t.server||a.login!==t.login||a.symbol!==t.symbol||
   a.currency!==t.currency||a.mode!==t.account_trade_mode||a.demo!==t.demo||
   r.context!==JSON.stringify([t.server,t.login,t.symbol,t.currency,t.account_trade_mode,t.demo]))
  return unavailable('بيانات التعلّم غير حديثة أو لا تطابق هوية حساب MT5 الحالي ونوعه');
 if(!count(r.snapshots)||!count(r.pending_labels)||
   !count(r.matured_schema_eligible_labels)||
   !count(r.matured_incompatible_schema_labels)||
   r.pending_labels+r.matured_schema_eligible_labels+r.matured_incompatible_schema_labels!==r.snapshots||
   (r.last_bar_time!==null&&(!count(r.last_bar_time)||r.last_bar_time===0)))
  return unavailable('أعداد العينات أو مخططات التدريب الصادرة عن Core غير متسقة');
 return {ready:true,reason:r.snapshots===0?'لم تُسجل شموع M1 لهذا الحساب بعد':'عينات M1 موثقة سياقياً من Core، وليست تداول MT5',
  mode:t.demo?'DEMO':'REAL',snapshots:r.snapshots,
  pending:r.pending_labels,eligible:r.matured_schema_eligible_labels,
  incompatible:r.matured_incompatible_schema_labels,
  lastBar:r.last_bar_time,trainingEnabled:false,predictionEnabled:false};
}
