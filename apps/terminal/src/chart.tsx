import {useEffect,useMemo,useRef,useState} from 'react';
import {GoldSymbol} from './gold-symbol';
import {Crosshair,Activity,ZoomIn,ZoomOut,RotateCcw,Maximize2,Minimize2,Target} from 'lucide-react';
import type {Candle,Market,Position} from './types';
import type {ChartFrameMeta} from './chart-history';
import {num} from './format';

const FRAMES=['m1','m5','m15','m30','h1','h4','d1'] as const;
const EMPTY_CANDLES:Candle[]=[];
export type ChartFrame=typeof FRAMES[number];
const UP='#0eaa78',DOWN='#e34855';
const clamp=(n:number,low:number,high:number)=>Math.max(low,Math.min(high,n));

/** Rendering never manufactures a candle. Reject malformed bars, order by time
 * and remove duplicate timestamps before calculating the display window. */
export function cleanChartCandles(input:unknown):Candle[]{
 if(!Array.isArray(input))return [];
 const sorted=input.filter((b):b is Candle=>Boolean(b&&typeof b==='object')&&Number.isSafeInteger(b.time)&&b.time>0&&
  [b.open,b.high,b.low,b.close].every(n=>Number.isFinite(n)&&n>0)&&
  b.high>=b.low&&b.high>=Math.max(b.open,b.close)&&b.low<=Math.min(b.open,b.close)&&
  Number.isSafeInteger(b.tick_volume)&&b.tick_volume>=0).sort((a,b)=>a.time-b.time);
 return sorted.filter((b,i)=>i===0||b.time!==sorted[i-1].time).slice(-300);
}
export function chartWindow(all:Candle[],count:number,offset:number):{bars:Candle[];start:number;end:number}{
 const end=clamp(all.length-Math.round(offset),0,all.length);
 const start=Math.max(0,end-clamp(Math.round(count),12,180));
 return {bars:all.slice(start,end),start,end};
}
export function emaSeries(bars:Candle[],period:number):number[]{
 const alpha=2/(period+1),values:number[]=[];
 for(let i=0;i<bars.length;i++)values.push(i===0?bars[i].close:bars[i].close*alpha+values[i-1]*(1-alpha));
 return values;
}
const priceText=(v:number,step:number)=>v.toFixed(step<.01?3:2);
function timeText(time:number,frame:ChartFrame){
 const date=new Date(time*1000);
 if(frame==='d1'||frame==='h4')return date.toLocaleDateString('en-GB',{timeZone:'UTC',day:'2-digit',month:'short'});
 return date.toLocaleTimeString('en-GB',{timeZone:'UTC',hour:'2-digit',minute:'2-digit'});
}
type ChartProps={market?:Market|null;symbol:string;quote?:number;positions?:Position[];frame?:ChartFrame;
 onFrameChange?:(frame:ChartFrame)=>void;theme?:'dark'|'smoke'|'light';frameMeta?:ChartFrameMeta;preview?:boolean};
type Hover={index:number;y:number}|null;
export default function TradingChart({market,symbol,quote,positions=[],frame:currentFrame,onFrameChange,
 theme='light',frameMeta,preview=false}:ChartProps){
 const [localFrame,setLocalFrame]=useState<ChartFrame>('m15');
 const frame=currentFrame??localFrame;
 const [averages,setAverages]=useState(true),[crosshair,setCrosshair]=useState(true);
 const [expanded,setExpanded]=useState(false),[fitLevels,setFitLevels]=useState(false);
 const [barsVisible,setBarsVisible]=useState(56),[offset,setOffset]=useState(0);
 const [hover,setHover]=useState<Hover>(null);
 const canvas=useRef<HTMLCanvasElement>(null);
 const drag=useRef<{x:number;offset:number;step:number}|null>(null);
 const frameCandles=(market?.[frame as keyof Market]??EMPTY_CANDLES) as Candle[];
 const all=useMemo(()=>cleanChartCandles(frameCandles),[frameCandles]);
 const windowed=useMemo(()=>chartWindow(all,barsVisible,offset),[all,barsVisible,offset]);
 const chart=windowed.bars,ready=chart.length>=3;
 const ema9=useMemo(()=>emaSeries(all,9),[all]),ema21=useMemo(()=>emaSeries(all,21),[all]);
 // A single broker-provided position is shown, never a guessed trading signal.
 const position=useMemo(()=>positions.find(p=>p.symbol===symbol&&
  Number.isFinite(p.price_open)&&p.price_open>0&&Number.isFinite(p.volume)&&p.volume>0&&
  (p.side.toUpperCase()==='BUY'||p.side.toUpperCase()==='SELL')),[positions,symbol]);
 const priceQuote=typeof quote==='number'&&Number.isFinite(quote)&&quote>0?quote:null;
 const setFrame=(next:ChartFrame)=>{
  setLocalFrame(next);onFrameChange?.(next);setHover(null);setOffset(0);drag.current=null;
 };
 const resetView=()=>{setOffset(0);setBarsVisible(56);setHover(null)};
 useEffect(()=>{
  setOffset(0);setHover(null);drag.current=null;
 },[frame,symbol]);
 useEffect(()=>{
  if(!expanded)return;
  const close=(event:KeyboardEvent)=>{if(event.key==='Escape')setExpanded(false)};
  window.addEventListener('keydown',close);return()=>window.removeEventListener('keydown',close);
 },[expanded]);
 // Browser wheel events may be passive under React. A native non-passive
 // listener enables chart zoom without scrolling the entire application.
 useEffect(()=>{
  const el=canvas.current;if(!el)return;
  const onWheel=(event:WheelEvent)=>{
   if(all.length<3)return;
   event.preventDefault();
   if(event.shiftKey)setOffset(v=>clamp(v+(event.deltaY>0?5:-5),0,Math.max(0,all.length-12)));
   else setBarsVisible(v=>clamp(v+(event.deltaY>0?8:-8),16,120));
  };
  el.addEventListener('wheel',onWheel,{passive:false});
  return()=>el.removeEventListener('wheel',onWheel);
 },[all.length]);
 useEffect(()=>{
  const el=canvas.current;if(!el)return;
  const ctx=el.getContext('2d');if(!ctx)return;
  const draw=()=>{
   const bounds=el.getBoundingClientRect();
   const w=Math.round(bounds.width),h=Math.round(bounds.height);
   if(w<160||h<100)return;
   const dpr=Math.min(window.devicePixelRatio||1,2);
   if(el.width!==Math.round(w*dpr)||el.height!==Math.round(h*dpr)){
    el.width=Math.round(w*dpr);el.height=Math.round(h*dpr);
   }
   ctx.setTransform(dpr,0,0,dpr,0,0);
   ctx.clearRect(0,0,w,h);
   const light=theme==='light',bg=light?'#ffffff':'#151e26',
     ink=light?'#71808b':'#a9bac5',grid=light?'#e9edf2':'#26343f',
     lineUp=light?'#079f72':'#22c492',lineDown=light?'#dc3c49':'#f35a69';
   const xLeft=12,xRight=w-69,top=12,timeBottom=h-23;
   const volHeight=clamp(h*.19,33,68),volumeBottom=timeBottom-7;
   const volumeTop=volumeBottom-volHeight,priceBottom=volumeTop-13;
   const priceHeight=priceBottom-top,plotWidth=xRight-xLeft;
   ctx.fillStyle=bg;ctx.fillRect(0,0,w,h);
   ctx.fillStyle=light?'#f5f8fb':'#19242d';ctx.fillRect(xRight,0,w-xRight,h);
   ctx.fillStyle=light?'#f9fbfd':'#17222b';ctx.fillRect(0,timeBottom,w,h-timeBottom);
   ctx.font='11px Segoe UI, sans-serif';ctx.textAlign='left';
   ctx.strokeStyle=grid;ctx.lineWidth=1;
   for(let j=0;j<=5;j++){
    const gy=Math.round(top+j*priceHeight/5)+.5;
    ctx.beginPath();ctx.moveTo(xLeft,gy);ctx.lineTo(w,gy);ctx.stroke();
   }
   for(let j=0;j<=6;j++){
    const gx=Math.round(xLeft+j*plotWidth/6)+.5;
    ctx.beginPath();ctx.moveTo(gx,top);ctx.lineTo(gx,timeBottom);ctx.stroke();
   }
   ctx.beginPath();ctx.moveTo(xLeft,volumeTop-.5);ctx.lineTo(w,volumeTop-.5);ctx.stroke();
   if(!ready){
    ctx.fillStyle=ink;ctx.font='13px Segoe UI, sans-serif';ctx.textAlign='center';
    ctx.fillText(preview?'لا توجد شموع معاينة':'بانتظار بيانات OHLC من MT5 لهذا الفريم',w/2,h/2);
    return;
   }
   const highC=Math.max(...chart.map(b=>b.high)),lowC=Math.min(...chart.map(b=>b.low));
   const baseRange=Math.max(.01,highC-lowC);
   const levels=position?[position.price_open,position.sl,position.tp]
    .filter((v):v is number=>Number.isFinite(v)&&v>0):[];
   // Include SL/TP when they are at a sensible visible distance. A far-away
   // target may not compress real candles into a one-pixel line.
   const levelSpan=Math.max(highC,...levels)-Math.min(lowC,...levels);
   const visibleLevels=fitLevels&&levelSpan<=baseRange*3
    ?levels
    :levels.filter(v=>v>lowC-baseRange*.2&&v<highC+baseRange*.2);
   const nearby=[...visibleLevels,priceQuote]
    .filter((v):v is number=>typeof v==='number'&&Number.isFinite(v)&&v>0&&
     v>lowC-baseRange*(fitLevels?.9:.3)&&v<highC+baseRange*(fitLevels?.9:.3));
   const low=Math.min(lowC,...nearby)-baseRange*.095;
   const high=Math.max(highC,...nearby)+baseRange*.095;
   const y=(price:number)=>top+(high-price)/(high-low)*priceHeight;
   const step=plotWidth/chart.length,bodyW=Math.max(2,Math.min(16,step*.72));
   const volumeMax=Math.max(1,...chart.map(b=>b.tick_volume));
   ctx.fillStyle=ink;ctx.textAlign='left';ctx.font='11px Segoe UI, sans-serif';
   for(let j=0;j<=5;j++){
    const value=high-(high-low)*j/5;
    ctx.fillText(priceText(value,baseRange/5),xRight+7,top+j*priceHeight/5+4);
   }
   // The lower histogram is MT5 tick volume, not centralized exchange volume.
   chart.forEach((b,i)=>{
    const x=xLeft+(i+.5)*step,up=b.close>=b.open;
    const color=up?(light?'rgba(18,166,116,.35)':'rgba(35,190,137,.42)'):
      (light?'rgba(219,70,84,.32)':'rgba(240,85,99,.42)');
    const barH=b.tick_volume/volumeMax*(volHeight-9);
    ctx.fillStyle=color;ctx.fillRect(Math.round(x-bodyW/2),volumeBottom-barH,bodyW,Math.max(1,barH));
   });
   // Broker position levels stay behind the candle bodies.
   if(position){
    const levels=[
     {value:position.tp,label:'TP',color:'#099e68'},
     {value:position.sl,label:'SL',color:'#dc4654'},
     {value:position.price_open,label:'ENTRY',color:'#b58a2a'}
    ];
    ctx.save();ctx.beginPath();ctx.rect(xLeft,top,plotWidth,priceHeight);ctx.clip();
    for(const l of levels){
     if(!Number.isFinite(l.value)||l.value<=low||l.value>=high)continue;
     const yy=y(l.value);
     ctx.fillStyle=l.color==='#099e68'?'rgba(13,167,103,.08)':
      l.color==='#dc4654'?'rgba(222,67,84,.07)':'rgba(181,138,42,.05)';
     ctx.fillRect(xLeft,yy-4,plotWidth,8);
     ctx.strokeStyle=l.color;ctx.lineWidth=1;ctx.setLineDash([5,4]);
     ctx.beginPath();ctx.moveTo(xLeft,yy);ctx.lineTo(xRight,yy);ctx.stroke();ctx.setLineDash([]);
    }
    ctx.restore();
    for(const l of levels){
     if(!Number.isFinite(l.value)||l.value<=low||l.value>=high)continue;
     const yy=y(l.value);
     ctx.fillStyle=l.color;ctx.fillRect(xRight-33,yy-11,33,15);
     ctx.fillStyle='#fff';ctx.textAlign='center';ctx.font='bold 9px Segoe UI';
     ctx.fillText(l.label,xRight-16,yy);ctx.textAlign='left';
    }
    // Off-screen levels are displayed as arrows, never projected at a false
    // price coordinate. The default price scale prioritizes candle detail.
    for(const l of levels){
     if(!Number.isFinite(l.value)||l.value<=0||l.value>=low&&l.value<=high)continue;
     const above=l.value>high;
     const badgeY=above?top+7:priceBottom-14;
     ctx.fillStyle=l.color;ctx.globalAlpha=.89;
     ctx.fillRect(xRight-110,badgeY,108,16);ctx.globalAlpha=1;
     ctx.fillStyle='#fff';ctx.font='bold 9px Segoe UI';ctx.textAlign='center';
     ctx.fillText((above?'↑ ':'↓ ')+l.label+' '+priceText(l.value,baseRange/5),
      xRight-56,badgeY+11);ctx.textAlign='left';
    }
   }
   // Render each OHLC bar as a wick plus filled body, independent of volume.
   ctx.save();ctx.beginPath();ctx.rect(xLeft,top,plotWidth,priceHeight);ctx.clip();
   chart.forEach((b,i)=>{
    const x=xLeft+(i+.5)*step,up=b.close>=b.open,col=up?lineUp:lineDown;
    const hi=y(b.high),lo=y(b.low),open=y(b.open),close=y(b.close);
    ctx.strokeStyle=col;ctx.lineWidth=1.25;
    ctx.beginPath();ctx.moveTo(Math.round(x)+.5,hi);ctx.lineTo(Math.round(x)+.5,lo);ctx.stroke();
    const bodyTop=Math.min(open,close),height=Math.abs(close-open);
    ctx.fillStyle=col;
    if(height<1.2)ctx.fillRect(Math.round(x-bodyW/2),bodyTop-.6,bodyW,1.3);
    else ctx.fillRect(Math.round(x-bodyW/2),bodyTop,bodyW,Math.max(1.2,height));
   });
   if(averages){
    for(const [array,color] of [[ema9,'#e4a83a'],[ema21,light?'#354f61':'#b0c0cb']] as const){
     ctx.strokeStyle=color;ctx.lineWidth=1.5;ctx.setLineDash([]);
     ctx.beginPath();let moved=false;
     for(let i=0;i<chart.length;i++){
      const v=array[windowed.start+i];if(!Number.isFinite(v))continue;
      const x=xLeft+(i+.5)*step;
      if(!moved){ctx.moveTo(x,y(v));moved=true}else ctx.lineTo(x,y(v));
     }
     if(moved)ctx.stroke();
    }
   }
   ctx.restore();
   // Last actual quote marker. Do not imply a live quote in DESIGN PREVIEW.
   if(priceQuote!==null&&priceQuote>low&&priceQuote<high){
    const yy=y(priceQuote),up=priceQuote>=chart[chart.length-1].open;
    ctx.save();ctx.setLineDash([3,3]);ctx.strokeStyle=up?UP:DOWN;
    ctx.beginPath();ctx.moveTo(xLeft,yy);ctx.lineTo(xRight,yy);ctx.stroke();ctx.restore();
    ctx.fillStyle=up?UP:DOWN;ctx.fillRect(xRight,yy-10,w-xRight,20);
    ctx.fillStyle='#fff';ctx.textAlign='center';ctx.font='bold 10px Segoe UI';
    ctx.fillText(priceText(priceQuote,baseRange/5),xRight+(w-xRight)/2,yy+3);
   }
   ctx.fillStyle=ink;ctx.textAlign='center';ctx.font='10px Segoe UI, sans-serif';
   const ticks=Math.min(6,chart.length-1);
   for(let j=0;j<=ticks;j++){
    const i=Math.round(j*(chart.length-1)/ticks),x=xLeft+(i+.5)*step;
    ctx.fillText(timeText(chart[i].time,frame),clamp(x,34,w-95),h-7);
   }
   ctx.textAlign='left';ctx.font='9px Segoe UI';ctx.fillStyle=ink;
   ctx.fillText('TICK VOL',xLeft+3,volumeTop+10);
   if(crosshair&&hover&&hover.index>=0&&hover.index<chart.length){
    const hx=xLeft+(hover.index+.5)*step,hy=clamp(hover.y,top,priceBottom);
    ctx.save();ctx.strokeStyle=light?'#78909ea6':'#a5bfcca0';ctx.lineWidth=1;
    ctx.setLineDash([4,4]);ctx.beginPath();
    ctx.moveTo(hx,top);ctx.lineTo(hx,timeBottom);
    ctx.moveTo(xLeft,hy);ctx.lineTo(xRight,hy);ctx.stroke();ctx.restore();
    const price=high-(hy-top)/priceHeight*(high-low);
    ctx.fillStyle=light?'#263b48':'#bacbd5';ctx.fillRect(xRight,hy-10,w-xRight,20);
    ctx.fillStyle=light?'#fff':'#15202b';ctx.font='bold 10px Segoe UI';ctx.textAlign='center';
    ctx.fillText(priceText(price,baseRange/5),xRight+(w-xRight)/2,hy+3);
   }
  };
  const observer=new ResizeObserver(draw);
  observer.observe(el);draw();return()=>observer.disconnect();
 },[chart,ready,windowed.start,ema9,ema21,position,priceQuote,theme,averages,crosshair,hover,frame,preview,fitLevels]);
 const highlighted=hover?chart[hover.index]??chart.at(-1):chart.at(-1);
 const historical=frame==='m30'||frame==='h1'||frame==='h4'||frame==='d1'?frameMeta?.[frame]:undefined;
 const dataLabel=preview?'SIMULATED • NOT MT5':historical?
   (historical.live?'MT5 HISTORY':'MT5 HISTORY • STALE'):'MT5 OHLC';
 return <section className={'panel chart-panel '+(expanded?'chart-expanded':'')}>
  <div className="chart-head">
   <div className="chart-symbol"><GoldSymbol className="gold-icon"/>
    <div><strong>{symbol||'XAUUSD'}</strong><small>{(symbol||'XAUUSD').toUpperCase().startsWith('XAU')?'Gold Spot / US Dollar':'Trading symbol'}</small></div>
    <b className="concept-chart-quote">{priceQuote===null?'—':num(priceQuote)}</b></div>
   <div className="chart-actions">
    <div className="timeframes">{FRAMES.map(tf=><button type="button" key={tf} className={tf===frame?'selected':''}
     aria-pressed={tf===frame} title={'MT5 '+tf.toUpperCase()+' • closed candles'}
     onClick={()=>setFrame(tf)}>{tf.toUpperCase()}</button>)}</div>
    <button type="button" className={'tool-button '+(crosshair?'tool-on':'')} title="Crosshair"
     aria-pressed={crosshair} onClick={()=>setCrosshair(v=>!v)}><Crosshair size={16}/></button>
    <button type="button" className={'tool-button '+(averages?'tool-on':'')} title="EMA 9 / EMA 21"
     aria-pressed={averages} onClick={()=>setAverages(v=>!v)}><Activity size={16}/></button>
    <button type="button" className="tool-button" title="تكبير الشموع" aria-label="تكبير الشموع"
     onClick={()=>setBarsVisible(v=>clamp(v-12,16,120))}><ZoomIn size={17}/></button>
    <button type="button" className="tool-button" title="تصغير الشموع" aria-label="تصغير الشموع"
     onClick={()=>setBarsVisible(v=>clamp(v+12,16,120))}><ZoomOut size={17}/></button>
    <button type="button" className="tool-button" title="العودة لأحدث شمعة" aria-label="العودة لأحدث شمعة"
     onClick={resetView}><RotateCcw size={16}/></button>
    <button type="button" className={'tool-button '+(fitLevels?'tool-on':'')}
     aria-label="ملاءمة مستويات الدخول والهدف والوقف" aria-pressed={fitLevels}
     title="إظهار مستويات SL/TP البعيدة ضمن تدريج السعر"
     disabled={!position} onClick={()=>setFitLevels(v=>!v)}><Target size={16}/></button>
    <button type="button" className="tool-button wide" aria-label={expanded?'استعادة حجم الشارت':'تكبير الشارت'}
     aria-pressed={expanded} title={expanded?'استعادة حجم الشارت':'تكبير الشارت'}
     onClick={()=>setExpanded(v=>!v)}>{expanded?<Minimize2 size={16}/>:<Maximize2 size={16}/>}</button>
   </div>
  </div>
  <div className="ohlc-line">{highlighted?
   'O '+num(highlighted.open)+'   H '+num(highlighted.high)+'   L '+num(highlighted.low)+
   '   C '+num(highlighted.close)+'   '+(highlighted.close>=highlighted.open?'▲':'▼')+
   '   Vol '+highlighted.tick_volume:'O —   H —   L —   C —'}
   <span className="concept-chart-source">{dataLabel}</span>
  </div>
  <div className="chart-frame">
   <canvas aria-label={'MT5 '+symbol+' '+frame+' candlestick chart'} title={'MT5 OHLC '+frame.toUpperCase()} ref={canvas}
    onDoubleClick={resetView}
    onPointerDown={e=>{
     if(e.button!==0||!ready)return;
     drag.current={x:e.clientX,offset,step:Math.max(2,(e.currentTarget.clientWidth-81)/chart.length)};
     e.currentTarget.setPointerCapture(e.pointerId);
    }}
    onPointerUp={e=>{drag.current=null;if(e.currentTarget.hasPointerCapture(e.pointerId))e.currentTarget.releasePointerCapture(e.pointerId)}}
    onPointerCancel={()=>{drag.current=null}}
    onPointerLeave={()=>{if(!drag.current)setHover(null)}}
    onPointerMove={e=>{
     if(!ready)return;
     if(drag.current){
      const shift=Math.round((e.clientX-drag.current.x)/drag.current.step);
      setOffset(clamp(drag.current.offset+shift,0,Math.max(0,all.length-12)));
      return;
     }
     if(!crosshair)return;
     const bounds=e.currentTarget.getBoundingClientRect();
     const width=Math.max(1,bounds.width-81),i=Math.floor((e.clientX-bounds.left-12)/width*chart.length);
     setHover({index:clamp(i,0,chart.length-1),y:e.clientY-bounds.top});
    }}/>
  </div>
  <div className="chart-footer">
   <span>{preview?'معاينة توضيحية، ليست أسعار السوق':'بيانات المصدر: MT5'} • {ready?chart.length+' شمعة '+frame.toUpperCase():'لا توجد شموع موثوقة'}{position?' • ENTRY / SL/TP من المركز المعروض':''}</span>
   <span>{offset>0?'عرض تاريخي · '+offset+' شمعة للخلف':'آخر شمعة'} • UTC • Tick Volume</span>
  </div>
 </section>;
}
