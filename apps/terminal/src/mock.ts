import type {Candle,Status} from './types';

// DISPLAY FIXTURE ONLY: deterministic pseudo-random OHLC makes the preview
// visually varied rather than the old perfect sine/cosine wave. No fixture
// ever enters Gateway, Core, or the MT5 execution paths.
export function generatePreviewCandles(seed=1,seconds=300):Candle[]{
 const series:Candle[]=[];
 let state=(Math.round(seed*1000003)>>>0)||1;
 const random=()=>{state=(Math.imul(state,1664525)+1013904223)>>>0;return state/4294967296};
 const gaussian=()=>random()+random()+random()+random()+random()+random()-3;
 let last=2471.2,volatility=.32;
 const count=128;
 // Keep fixture history in the past for every timeframe, including D1.
 const end=Math.floor(Date.UTC(2026,9,29,16,0,0)/1000/seconds)*seconds;
 for(let i=0;i<count;i++){
  const regime=i<28?-.065:i<54?.105:i<73?-.04:i<102?.15:.02;
  const shock=i%37===18?1.45:i%31===10?.75:0;
  volatility=Math.max(.20,Math.min(.95,volatility*.86+
   .13*(.22+Math.abs(gaussian())*.28+shock)));
  const gap=i%41===0&&i>0?gaussian()*.12:gaussian()*.025;
  const open=last+gap;
  const move=regime+gaussian()*volatility+(i%48===28?-.65:0);
  const close=open+move;
  const upper=.035+random()*volatility*.8+(i%23===7?.25:0);
  const lower=.035+random()*volatility*.75+(i%29===11?.21:0);
  const high=Math.max(open,close)+upper,low=Math.min(open,close)-lower;
  const tickVolume=Math.max(1,Math.round(35+random()*36+
   Math.abs(move)*35/Math.max(.2,volatility)+(i%40<9?24:0)));
  series.push({time:end-(count-1-i)*seconds,open,high,low,close,tick_volume:tickVolume});
  last=close;
 }
 // Visual-only reference price is aligned with the preview quote; this does
 // not change the OHLC relationships or convert it into a genuine feed.
 const shift=2479.54-series[series.length-1].close;
 return series.map(b=>({
  ...b,open:+(b.open+shift).toFixed(3),high:+(b.high+shift).toFixed(3),
  low:+(b.low+shift).toFixed(3),close:+(b.close+shift).toFixed(3)
 }));
}
export const DESIGN_FIXTURE:Status={
 version:'DESIGN FIXTURE',state:'MONITORING',requested_run:true,gateway_fresh:true,execution_enabled:false,
 telegram_status:'design preview',message:'إحصائيات توضيحية وليست مأخوذة من MT5',
 analysis:{regime:'UPTREND',action:'WAIT',current:false},
 telemetry:{connected:true,demo:true,login:0,server:'Preview • No Broker',currency:'USD',symbol:'XAUUSD',
  balance:51200,equity:52430.18,margin_free:46109.73,today_net:1230.18,
  tick_age_ms:28,bid:2479.54,ask:2479.56,
  positions:[
   {ticket:10342,symbol:'XAUUSD',side:'Buy',volume:.5,price_open:2476.12,price_current:2479.56,profit:172,sl:2472.5,tp:2486.8},
   {ticket:10341,symbol:'GBPUSD',side:'Sell',volume:1,price_open:1.2854,price_current:1.2841,profit:130,sl:1.289,tp:1.278},
   {ticket:10338,symbol:'NAS100',side:'Buy',volume:.2,price_open:19430.5,price_current:19412.3,profit:-36.4,sl:19360,tp:19600}
  ],
  market:{
   m1:generatePreviewCandles(1.15,60),m5:generatePreviewCandles(2.4,300),
   m15:generatePreviewCandles(3.8,900),m30:generatePreviewCandles(4.1,1800),
   h1:generatePreviewCandles(5.6,3600),h4:generatePreviewCandles(6.3,14400),
   d1:generatePreviewCandles(7.9,86400)
  }
 }
};
