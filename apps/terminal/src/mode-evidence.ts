import type {Status} from './types';

export type FourModeEvidenceRow={
 mode:'sniper'|'scalping'|'swing'|'expert';
 state:'descriptive_only'|'insufficient_or_contaminated';
 reason:string;
 samples:number;invalid:number;duplicates:number;
 winRate:number|null;net:number|null;
};
export type FourModeEvidenceView={
 available:boolean;reason:string;rows:FourModeEvidenceRow[];
 collisions:number;malformed:number;
};

const modes=['sniper','scalping','swing','expert'] as const;
const number=(v:unknown)=>typeof v==='number'&&Number.isFinite(v)?v:null;
const record=(v:unknown):Record<string,unknown>=>v!==null&&typeof v==='object'&&!Array.isArray(v)?v as Record<string,unknown>:{};

export function fourModeObservation(status:Status|null,report:unknown,now:number):FourModeEvidenceView{
 const empty=(reason:string):FourModeEvidenceView=>({available:false,reason,rows:[],collisions:0,malformed:0});
 const t=status?.telemetry;
 if(!status?.gateway_fresh||!t?.connected||!t.demo||t.account_trade_mode!==0||!Number.isSafeInteger(t.login)||t.login<=0)
  return empty('يتطلب حساب MT5 Demo متصلاً وحديثاً؛ لا ننقل إحصاءات من REAL أو حساب سابق');
 const r=record(report),a=record(r.account);
 const at=number(r.observed_ms);
 if(r.schema!=='p1h-four-mode-evidence-v1'||r.state!=='observed'||
     r.read_only!==true||r.automatic_promotion!==false||r.broker_execution_permission!==false||
     r.verified_demo_account!==true||r.independent_oos!==false||
     at===null||at>now+2000||now-at>15000||
     a.mode!==0||a.login!==t.login||a.server!==t.server||
     a.symbol!==t.symbol||a.currency!==t.currency||
     r.context!==JSON.stringify([t.server,t.login,t.symbol,t.currency]))
  return empty('بيانات الأوضاع الأربعة قديمة أو لا تطابق هوية حساب Demo الحالي');
 const collisions=number(r.cross_mode_position_collisions),malformed=number(r.malformed_broker_samples);
 if(collisions===null||malformed===null||collisions<0||malformed<0||!Number.isInteger(collisions)||!Number.isInteger(malformed))
  return empty('حالة مصدر دليل الصفقات غير مؤكدة');
 if(!Array.isArray(r.modes)||r.modes.length!==4)return empty('قائمة الأوضاع غير مكتملة');
 const rows:FourModeEvidenceRow[]=[];
 for(let i=0;i<4;i++){
  const v=record(r.modes[i]);
  if(v.mode!==modes[i]||v.source!=='reconciled_closed_mt5_demo_coordinator_samples'
    ||v.online_learning_could_have_seen_these_results!==true
    ||v.independent_oos!==false||v.promotion_authorized!==false
    ||v.minimum_samples!==30||v.sample_cap_per_mode!==240)
   return empty('تقرير الوضع لا يثبت مصدر النتائج أو شروط المقارنة');
  const samples=number(v.closed_costed_samples),invalid=number(v.invalid_samples),duplicates=number(v.duplicate_positions);
  if([samples,invalid,duplicates].some(n=>n===null||n!<0||!Number.isSafeInteger(n)))return empty('عداد عينة غير صالح');
  const state=v.status;
  if(state!=='descriptive_only'&&state!=='insufficient_or_contaminated')
   return empty('تصنيف دليل التداول غير معروف');
  if(state==='descriptive_only'&&(samples!<30||invalid!==0||duplicates!==0||collisions!==0||malformed!==0))
   return empty('تصنيف وصفي غير مسنود بعدد أو سلامة السجلات');
  const win=number(v.win_rate_after_costs),net=number(v.net_after_costs);
  if(win!==null&&(win<0||win>1))return empty('نسبة النجاح غير صالحة');
  if(state!=='descriptive_only'&&win!==null)return empty('نسبة نجاح على عينات غير مؤهلة');
  rows.push({
   mode:modes[i],state,
   reason:String(v.reason||'insufficient_closed_demo_deals').slice(0,80),
   samples:samples!,invalid:invalid!,duplicates:duplicates!,
   winRate:state==='descriptive_only'?win:null,
   net:net,
  });
 }
 return {available:true,reason:'read_only_demo_history_not_independent_oos',rows,collisions,malformed};
}
