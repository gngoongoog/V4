// P1-D: read-only small-account estimates. Authoritative limits, leverage,
// stop risk and order acceptance remain with Rust Core and the MT5 broker.
import {microAttestation} from './micro-readiness';

export type MicroBrokerTelemetry={
 connected?:boolean; demo?:boolean; account_trade_mode?:number|null;login?:number;server?:string;
 equity?:number|null;balance?:number|null;currency?:string|null;
 leverage?:number|null;margin_free?:number|null;
 minimum_lot_margin?:number|null;volume_min?:number|null;
};
export type MicroStatus={gateway_fresh?:boolean;telemetry?:MicroBrokerTelemetry|null};
export type MicroRisk={
 risk_percent?:number;micro_demo_risk_opt_in?:boolean;
 micro_real_temporary?:boolean;micro_real_cap_percent?:number;
 micro_real_recovery_equity?:number;daily_breaker_real_enabled?:boolean;
};
export type MicroView={
 available:boolean;accountType:'DEMO'|'REAL'|'UNVERIFIED';login:number|null;server:string;currency:string;
 equity:number|null;leverage:number|null;freeMargin:number|null;
 minimumLot:number|null;minimumLotMargin:number|null;
 marginCovered:boolean|null;scenarioMargin:number|null;
 budget:number|null;budgetPercent:number|null;
 microCandidate:boolean;note:string;
};
const positive=(n:unknown):n is number=>typeof n==='number'&&Number.isFinite(n)&&n>0;
const nonNegative=(n:unknown):n is number=>typeof n==='number'&&Number.isFinite(n)&&n>=0;
const cap=(n:number)=>Math.round(n*10000)/10000;
export function smallAccountPreview(status:MicroStatus|null,risk:MicroRisk|null,scenarioLeverage:number,monitor:unknown=null,nowMs=Date.now()):MicroView{
 const t=status?.telemetry;
 const base:MicroView={available:false,accountType:'UNVERIFIED',login:null,server:'',currency:'',
  equity:null,leverage:null,freeMargin:null,minimumLot:null,minimumLotMargin:null,
  marginCovered:null,scenarioMargin:null,budget:null,budgetPercent:null,microCandidate:false,
  note:'بانتظار قراءة MT5 حديثة لحساب محدد'};
 if(status?.gateway_fresh!==true||t?.connected!==true||!positive(t.equity)||
    !Number.isSafeInteger(t.login)||!positive(t.login)||!t.server||
    !((t.demo===true&&t.account_trade_mode===0)||(t.demo===false&&t.account_trade_mode===2)))return base;
 const actualLeverage=positive(t.leverage)&&Number.isInteger(t.leverage)?t.leverage:null;
 const freeMargin=nonNegative(t.margin_free)?t.margin_free:null;
 const minimumLot=positive(t.volume_min)?t.volume_min:null;
 const minimumLotMargin=nonNegative(t.minimum_lot_margin)?t.minimum_lot_margin:null;
 const isReal=t.account_trade_mode===2;
 // Rust's RiskConfig omits the default Micro cap (10) and recovery
 // milestone (500) from serialized config.json. Absence is NOT unknown
 // when a valid risk policy object is present.
 const recovery=risk?(risk.micro_real_recovery_equity===undefined?500:
    positive(risk.micro_real_recovery_equity)?risk.micro_real_recovery_equity:null):null;
 // This is a candidate by equity only. The irrevocable broker/Core recovery
 // history may already have disabled micro trading; never label it armed.
 const audit=microAttestation(status,monitor,nowMs);
 const microCandidate=Boolean(isReal&&risk?.micro_real_temporary===true&&
    recovery!==null&&t.equity<recovery&&audit.verified&&audit.candidate);
 // With a configured REAL Micro policy but no current Core recovery attestation
 // even 0.25% is a guess about the budget Core will send to MT5.
 const uncertainMicro=isReal&&risk?.micro_real_temporary===true&&!audit.verified;
 const rawPercent=uncertainMicro?null:microCandidate?
    audit.capPercent:risk?.risk_percent;
 const budgetPercent=positive(rawPercent)&&rawPercent<=20?rawPercent:null;
 const scenarioMargin=actualLeverage!==null&&minimumLotMargin!==null&&
    Number.isInteger(scenarioLeverage)&&scenarioLeverage>=1&&scenarioLeverage<=10000?
    cap(minimumLotMargin*actualLeverage/scenarioLeverage):null;
 return {...base,available:true,accountType:isReal?'REAL':'DEMO',login:t.login??null,server:t.server||'',currency:t.currency||'',
    equity:t.equity,leverage:actualLeverage,freeMargin,minimumLot,minimumLotMargin,
    marginCovered:freeMargin!==null&&minimumLotMargin!==null?freeMargin>=minimumLotMargin:null,
    scenarioMargin,budgetPercent,budget:budgetPercent!==null?cap(t.equity*budgetPercent/100):null,
    microCandidate,
    note:uncertainMicro?'حالة Micro REAL غير مؤكدة: يلزم تقرير Core مطابق وحديث قبل عرض أي ميزانية':
      microCandidate?'مرشح لوضع Micro حسب سجل Core الحالي؛ لا يعتبر إذناً بفتح صفقة':
      'ميزانية مخاطرة عادية تقديرية؛ لا تمثل إذناً بالتداول أو ضمان هامش'};
}
export type PortfolioSettings={enabled:boolean;max_positions:number;max_lots:number;max_risk_percent:number};
export function validatePortfolio(value:unknown):PortfolioSettings{
 if(!value||typeof value!=='object'||Array.isArray(value))throw Error('إعدادات المحفظة غير صالحة');
 const v=value as Record<string,unknown>;
 if(Object.keys(v).sort().join(',')!==['enabled','max_lots','max_positions','max_risk_percent'].sort().join(',')||
    typeof v.enabled!=='boolean'||!Number.isInteger(v.max_positions)||
    (v.max_positions as number)<1||(v.max_positions as number)>100||
    !positive(v.max_lots)||v.max_lots<0.001||v.max_lots>10000||
    !positive(v.max_risk_percent)||v.max_risk_percent<0.01||v.max_risk_percent>100){
   throw Error('حدود المحفظة: المراكز 1–100؛ اللوت 0.001–10000؛ المخاطرة 0.01–100%');
 }
 return {enabled:v.enabled,max_positions:v.max_positions as number,
  max_lots:v.max_lots as number,max_risk_percent:v.max_risk_percent as number};
}
