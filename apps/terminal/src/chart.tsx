import {useEffect,useMemo,useRef,useState} from 'react';
import {GoldSymbol} from './gold-symbol';
import type {Candle,Market} from './types';
import type {ChartFrameMeta} from './chart-history';
import {num} from './format';
const FRAMES=['m1','m5','m15','m30','h1','h4','d1'] as const;
export type ChartFrame=typeof FRAMES[number];
export default function TradingChart({market,symbol,quote,frame:currentFrame,onFrameChange,theme='light',frameMeta,preview=false}:{market?:Market|null;symbol:string;quote?:number;frame?:ChartFrame;onFrameChange?:(frame:ChartFrame)=>void;theme?:'dark'|'smoke'|'light';frameMeta?:ChartFrameMeta;preview?:boolean}){
 const [localFrame,setLocalFrame]=useState<ChartFrame>('m15');const frame=currentFrame??localFrame;
 const setFrame=(v:ChartFrame)=>{setLocalFrame(v);onFrameChange?.(v)};
 const [averages,setAverages]=useState(true);const [crosshair,setCrosshair]=useState(true);
 const canvas=useRef<HTMLCanvasElement>(null);const [hover,setHover]=useState<number|null>(null);const [zoom,setZoom]=useState(68);
 const candles=(market?.[frame as keyof Market]??[]) as Candle[];
 const chart=useMemo(()=>candles.filter(c=>[c.open,c.close,c.high,c.low].every(Number.isFinite)&&c.high>=Math.max(c.open,c.close)&&c.low<=Math.min(c.open,c.close)&&c.tick_volume>=0).slice(-zoom),[candles,zoom]);
 const ready=chart.length>=3;
 useEffect(()=>{
  const el=canvas.current;if(!el)return;
  const context=el.getContext('2d');if(!context)return;
  let w=0,h=0;
  const paint=()=>{
    const bounds=el.getBoundingClientRect();w=Math.round(bounds.width);h=Math.round(bounds.height);
    if(!w||!h)return;
    const dpr=Math.min(window.devicePixelRatio||1,2);el.width=w*dpr;el.height=h*dpr;
    context.setTransform(dpr,0,0,dpr,0,0);context.clearRect(0,0,w,h);
    const light=theme==='light';
    context.fillStyle=light?'#f8fafc':'#141c23';context.fillRect(0,0,w,h);
    const left=10,right=w-63,top=10,bottom=h-36,ph=bottom-top-35,volBase=bottom,volH=45;
    context.strokeStyle=light?'rgba(121,140,153,.16)':'rgba(139,158,174,.16)';context.lineWidth=1;
    for(let j=0;j<=5;j++){const y=top+j*(bottom-top)/5;context.beginPath();context.moveTo(left,y);context.lineTo(w,y);context.stroke()}
    for(let j=0;j<10;j++){const x=left+j*(right-left)/9;context.beginPath();context.moveTo(x,top);context.lineTo(x,bottom);context.stroke()}
    if(!ready){context.fillStyle=light?'#73818b':'#a9bac4';context.font='13px Segoe UI';context.textAlign='center';context.fillText('بانتظار شموع MT5 مكتملة لهذه الفترة',w/2,h/2);return}
    const lo=Math.min(...chart.map(c=>c.low)),hi=Math.max(...chart.map(c=>c.high));const padding=Math.max((hi-lo)*.09,.01);const low=lo-padding,high=hi+padding;
    const y=(v:number)=>top+(high-v)/(high-low)*ph;
    const dx=(right-left)/chart.length;const bodyWidth=Math.max(2,Math.min(12,dx*.65));
    const volumeMax=Math.max(1,...chart.map(c=>c.tick_volume||0));
    chart.forEach((bar,i)=>{
       const x=left+(i+.5)*dx,up=bar.close>=bar.open;
       const color=up?'#129a60':'#df3742';
       context.fillStyle=up?'#159f6460':'#dc475660';
       context.fillRect(x-bodyWidth/2,volBase-(bar.tick_volume||0)/volumeMax*volH,bodyWidth,Math.max(1,(bar.tick_volume||0)/volumeMax*volH));
       context.strokeStyle=color;context.lineWidth=1.35;context.beginPath();context.moveTo(x,y(bar.high));context.lineTo(x,y(bar.low));context.stroke();
       context.fillStyle=color;context.fillRect(x-bodyWidth/2,y(Math.max(bar.open,bar.close)),bodyWidth,Math.max(1,Math.abs(y(bar.open)-y(bar.close))));
    });
    if(averages){
      for (const [period,color] of [[11,'#5d7d79'],[25,'#343e44']] as const){
        context.strokeStyle=color;context.lineWidth=1.4;context.beginPath();
        chart.forEach((_,i)=>{if(i<period-1)return;const avg=chart.slice(i-period+1,i+1).reduce((s,c)=>s+c.close,0)/period;
          const x=left+(i+.5)*dx;if(i===period-1)context.moveTo(x,y(avg));else context.lineTo(x,y(avg))});context.stroke();
      }
    }
    context.textAlign='left';context.fillStyle=light?'#6e7a84':'#aabac5';context.font='11px Segoe UI';
    for(let j=0;j<6;j++)context.fillText(num(high-(high-low)*j/5,2),right+7,top+j*ph/5+4);
    for(let j=0;j<7;j++){const bar=chart[Math.min(chart.length-1,Math.floor(j*(chart.length-1)/6))];const t=new Date(bar.time*1000);const label=frame==='h4'||frame==='d1'?t.toLocaleDateString('en-GB',{day:'2-digit',month:'short',timeZone:'UTC'}):t.toLocaleTimeString('en-GB',{hour:'2-digit',minute:'2-digit',timeZone:'UTC'});context.fillText(label,left+j*(right-left)/7,bottom+22)}
    if(crosshair&&hover!==null&&hover>=0&&hover<chart.length){
      const x=left+(hover+.5)*dx;context.save();context.setLineDash([4,4]);context.strokeStyle='#9fe7fe9c';context.beginPath();context.moveTo(x,top);context.lineTo(x,bottom);context.stroke();context.restore();
    }
  };
  const observer=new ResizeObserver(paint);observer.observe(el);paint();return ()=>observer.disconnect();
 },[chart,averages,crosshair,hover,ready,theme,frame]);
 const last=chart.at(-1);const highlighted=hover===null?last:chart[hover]??last;
 const historical=frame==='m30'||frame==='h1'||frame==='h4'||frame==='d1'?frameMeta?.[frame]:undefined;
 return <section className="panel chart-panel">
   <div className="chart-head"><div className="chart-symbol"><GoldSymbol className="gold-icon"/><div><strong>{symbol||'XAUUSD'}</strong><small>{(symbol||'XAUUSD').toUpperCase().startsWith('XAU')?'Gold Spot / US Dollar':'Trading symbol'}</small></div><b className="concept-chart-quote">{quote===undefined?'—':num(quote)}</b></div>
    <div className="chart-actions"><div className="timeframes">{FRAMES.map(tf=><button key={tf} type="button" className={tf===frame?'selected':''} aria-pressed={tf===frame} title={`MT5 ${tf.toUpperCase()} • closed candles`} onClick={()=>{setFrame(tf);setHover(null)}}>{tf.toUpperCase()}</button>)}</div>
      <button className={'tool-button '+(crosshair?'tool-on':'')} onClick={()=>setCrosshair(v=>!v)} title="Crosshair">⌖</button>
      <button className={'tool-button '+(averages?'tool-on':'')} onClick={()=>setAverages(v=>!v)} title="Moving averages">⌁</button>
      <button className="tool-button" onClick={()=>setZoom(v=>v===68?40:v===40?84:68)} title="Candle zoom">⊕</button>
    </div></div>
    <div className="ohlc-line">{highlighted?`O ${num(highlighted.open)}　H ${num(highlighted.high)}　L ${num(highlighted.low)}　C ${num(highlighted.close)}　${highlighted.close>=highlighted.open?'▲':'▼'}`:'O —　H —　L —　C —'}</div>
    <div className="chart-frame"><canvas aria-label={`MT5 ${symbol} ${frame} candlestick chart`} ref={canvas} onMouseLeave={()=>setHover(null)} onMouseMove={e=>{
      if(!crosshair||!ready)return;
      const width=e.currentTarget.getBoundingClientRect().width-73;setHover(Math.max(0,Math.min(chart.length-1,Math.floor((e.nativeEvent.offsetX-10)/width*chart.length))))
    }}/></div>
    <div className="chart-footer">{preview?'DESIGN PREVIEW':historical?(historical.live?'MT5 OHLC':'MT5 HISTORY · STALE'):'MT5 OHLC'} • {ready?`${chart.length} bars • ${frame.toUpperCase()} · UTC`:'No verified candles'} <span>{quote===undefined?'':`LAST ${num(quote)}`}</span></div>
   </section>
}
