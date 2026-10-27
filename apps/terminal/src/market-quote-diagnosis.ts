import type {Status} from './types';
import type {FeedEvidence} from './feed';

// A broker account can remain connected while its symbol is unavailable or
// its last quote is stale (weekend/holiday/network). Never infer a market
// session schedule, auto-select a different symbol, or authorize trading.
export type MarketQuoteDiagnosis={
 code:'CORE_OFFLINE'|'BROKER_OFFLINE'|'SYMBOL_MISSING'|'NO_QUOTE'|'UNVERIFIED_CLOCK'|'STALE_QUOTE'|'FRESH';
 detail:string;short:string;settingsRequired:boolean;
};
function ageDescription(ms:number):string{
 if(ms>=3600000)return `${Math.floor(ms/3600000)} ساعة`;
 if(ms>=60000)return `${Math.floor(ms/60000)} دقيقة`;
 return `${Math.floor(ms/1000)} ثانية`;
}
export function diagnoseMarketQuote(status:Status|null,e:FeedEvidence):MarketQuoteDiagnosis{
 if(!e.core)return {code:'CORE_OFFLINE',detail:'خدمة Rust Core غير متاحة؛ لا يمكن التحقق من الأسعار',short:'CORE OFFLINE',settingsRequired:false};
 if(!e.broker)return {code:'BROKER_OFFLINE',detail:'الاتصال بحساب MT5 غير مؤكد؛ لا توجد أسعار قابلة للاعتماد',short:'MT5 OFFLINE',settingsRequired:false};
 const t=status?.telemetry;
 if(e.quote)return {code:'FRESH',detail:`السعر حديث: ${Math.round(e.quoteAgeMs??0)} ms`,short:'QUOTES FRESH',settingsRequired:false};
 const symbol=t?.symbol||'';
 const error=t?.error||'';
 const missing=error.match(/^Symbol (.+?) is unavailable; choose its exact broker name in Settings\./);
 if(missing&&missing[1]===symbol)return {
   code:'SYMBOL_MISSING',
   detail:`الرمز ${symbol} غير موجود في حساب الوسيط. تحقق من الاسم الحرفي داخل Market Watch ثم إعدادات BRAINTRADE؛ لن نبدّل الرمز تلقائياً.`,
   short:'SYMBOL UNAVAILABLE',settingsRequired:true,
 };
 if(e.quoteAgeMs!==null&&Number.isFinite(e.quoteAgeMs)&&e.quoteAgeMs>3000)return {
   code:'STALE_QUOTE',detail:`آخر سعر للرمز ${symbol||'الحالي'} عمره ${ageDescription(e.quoteAgeMs)}. ربما السوق مغلق، لكن لا يمكن إثبات السبب من قِدم السعر وحده. التداول يبقى محجوباً.`,
   short:'QUOTES STALE',settingsRequired:false,
 };
 if(t?.bid!=null||t?.ask!=null)return {
   code:'UNVERIFIED_CLOCK',detail:'توجد أسعار بدون توقيت Broker صالح وحديث؛ ممنوع اعتبارها أسعار تنفيذ.',
   short:'QUOTE CLOCK UNKNOWN',settingsRequired:false,
 };
 return {
   code:'NO_QUOTE',detail:`حساب MT5 متصل، لكن لا توجد أسعار موثوقة للرمز ${symbol||'المختار'}. تحقق من اسم الرمز وتغذيته ولا تشغّل التداول.`,
   short:'NO VERIFIED QUOTE',settingsRequired:false,
 };
}
