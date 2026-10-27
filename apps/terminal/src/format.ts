import type {Telemetry} from './types';
export const valid=(x:unknown):x is number=>typeof x==='number'&&Number.isFinite(x);
export const num=(n:unknown,digits=2)=>valid(n)?n.toLocaleString('en-US',{minimumFractionDigits:digits,maximumFractionDigits:digits}):'—';
export function money(n:unknown,currency='USD'){
 if(!valid(n))return '—';
 const mark:Record<string,string>={USD:'$',EUR:'€',GBP:'£',IQD:'د.ع',JPY:'¥',AED:'د.إ'};
 const minor=currency==='IQD'||currency==='JPY'?0:2;
 const value=num(Math.abs(n),minor);
 return `${n<0?'-':''}${mark[currency]??currency}${value}`;
}
export const signed=(v:unknown,c:string)=>valid(v)?`${v>=0?'+':''}${money(v,c)}`:'—';
export const up=(p:unknown)=>typeof p==='string'&&/buy/i.test(p);
export const connected=(t:Telemetry|null|undefined,fresh:boolean)=>Boolean(t?.connected&&fresh);
