import type {Candle,Market,Telemetry} from './types';

type ExtraFrame='m30'|'h1'|'h4'|'d1';
export type ChartFrameMeta=Partial<Record<ExtraFrame,{capturedMs:number;live:boolean}>>;
export type ChartHistory={market:Market;meta:ChartFrameMeta};
const object=(v:unknown):Record<string,unknown>=>v&&typeof v==='object'&&!Array.isArray(v)?v as Record<string,unknown>:{};

// This is display-only history. Require the current account identity even if
// a previous IPC call returns late after the user changed MT5 accounts.
export function chartHistory(input:unknown,account:Telemetry|null,now:number):ChartHistory{
 const empty:ChartHistory={market:{},meta:{}};
 const root=object(input),who=object(root.account),frames=object(root.frames);
 if(!account||!account.connected||root.schema!=='chart-frames-v1'||root.ready!==true||root.read_only!==true||
    !Number.isSafeInteger(account.login)||account.login<=0||!account.server||!account.symbol||!account.currency||
    who.login!==account.login||who.server!==account.server||who.symbol!==account.symbol||
    who.currency!==account.currency||who.trade_mode!==(account.account_trade_mode??null))return empty;
 const result:ChartHistory={market:{},meta:{}};
 for(const name of ['m30','h1','h4','d1'] as const){
  const data=object(frames[name]),bars=data.bars;
  if(!Array.isArray(bars)||bars.length<3||bars.length>180||typeof data.captured_ms!=='number'||
     !Number.isFinite(data.captured_ms)||data.captured_ms<=0||data.captured_ms>now+2000||typeof data.live!=='boolean')continue;
  let previous=0;const candles:Candle[]=[];let valid=true;
  for(const candidate of bars){
   const b=object(candidate);
   if(typeof b.time!=='number'||!Number.isSafeInteger(b.time)||b.time<=previous||b.time*1000>data.captured_ms+2000||
      !['open','high','low','close'].every(k=>typeof b[k]==='number'&&Number.isFinite(b[k])&&(b[k] as number)>0)||
      typeof b.tick_volume!=='number'||!Number.isSafeInteger(b.tick_volume)||b.tick_volume<0||
      (b.low as number)>Math.min(b.open as number,b.close as number)||
      (b.high as number)<Math.max(b.open as number,b.close as number)){valid=false;break}
   previous=b.time;candles.push(b as unknown as Candle);
  }
  if(valid){result.market[name]=candles;result.meta[name]={capturedMs:data.captured_ms,live:data.live&&now>=data.captured_ms&&now-data.captured_ms<120000}}
 }
 return result;
}
