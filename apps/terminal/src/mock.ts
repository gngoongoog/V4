import type {Candle,Status} from './types';
// Visual fixture ONLY. Never used to start trading or as an actual broker quote.
const generate=(offset=0):Candle[]=>{
  const values:Candle[]=[];let price=2478.32;
  for(let i=0;i<86;i++){
    const wave=Math.sin(i*.19+offset)*1.8+Math.sin(i*.47+offset)*.9+Math.cos(i*.08)*1.1;
    const close=2477.4+wave+0.022*i;
    const wick=.18+Math.abs(Math.sin(i*.65))*.45;
    values.push({time:1782000000+i*300,open:price,close,high:Math.max(price,close)+wick,low:Math.min(price,close)-wick,tick_volume:Math.round(12+35*Math.abs(Math.sin(i*.7)))});
    price=close;
  }
  return values;
};
export const DESIGN_FIXTURE:Status={
  version:'DESIGN FIXTURE',state:'MONITORING',requested_run:true,gateway_fresh:true,execution_enabled:false,
  telegram_status:'design preview',message:'إحصائيات توضيحية وليست مأخوذة من MT5',
  analysis:{regime:'UPTREND',action:'WAIT',current:false},
  telemetry:{connected:true,demo:true,login:0,server:'Preview • No Broker',currency:'USD',symbol:'XAUUSD',balance:51200,equity:52430.18,margin_free:46109.73,today_net:1230.18,tick_age_ms:28,bid:2479.54,ask:2479.56,
    positions:[{ticket:10342,symbol:'XAUUSD',side:'Buy',volume:.5,price_open:2476.12,price_current:2479.56,profit:172,sl:2472.5,tp:2486.8},{ticket:10341,symbol:'GBPUSD',side:'Sell',volume:1,price_open:1.2854,price_current:1.2841,profit:130,sl:1.289,tp:1.278},{ticket:10338,symbol:'NAS100',side:'Buy',volume:.2,price_open:19430.5,price_current:19412.3,profit:-36.4,sl:19360,tp:19600}],
    market:{m1:generate(0.15),m5:generate(.5),m15:generate(1.1)}
  }
};
