// Exercise the real chart controls without requiring a browser or broker.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require('typescript'),root=path.resolve(__dirname,'../src');
const source=fs.readFileSync(path.join(root,'chart.tsx'),'utf8');
const js=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX}}).outputText;
const jsx=(type,props)=>({type,props:props||{}}),out={};
vm.runInNewContext(js,{exports:out,require(id){
 if(id==='react')return {useState:v=>[v,()=>{}],useEffect:()=>{},useMemo:fn=>fn(),useRef:()=>({current:null})};
 if(id==='react/jsx-runtime')return {jsx,jsxs:jsx};
 if(id==='./gold-symbol')return {GoldSymbol:'GoldSymbol'};
 if(id==='lucide-react')return Object.fromEntries(['Crosshair','Activity','ZoomIn','ZoomOut','RotateCcw','Maximize2','Minimize2','Target'].map(k=>[k,k]));
 if(id==='./format')return {num:v=>String(v)};
 throw Error('Unknown chart dependency '+id);
},Date,Math,Number,String});
function nodes(n,out=[]){if(!n||typeof n!=='object')return out;if(Array.isArray(n)){n.forEach(v=>nodes(v,out));return out}out.push(n);nodes(n.props?.children,out);return out}
let chosen='';const chart=out.default({symbol:'XAUUSD',frame:'m15',onFrameChange:v=>chosen=v,market:{}});
const buttons=nodes(chart).filter(n=>n.type==='button'&&n.props.title?.includes('closed candles'));
assert.equal(buttons.length,7);
for(const [i,frame] of ['m1','m5','m15','m30','h1','h4','d1'].entries()){
 assert(!buttons[i].props.disabled,frame+' must be selectable even when history has not arrived');
 buttons[i].props.onClick();assert.equal(chosen,frame);
}
const expand=nodes(chart).find(n=>n.type==='button'&&n.props['aria-label']==='تكبير الشارت');
assert(expand&&!expand.props.disabled,'Chart expansion must have an active accessible control');

const position={ticket:1,symbol:'XAUUSD',side:'BUY',volume:.1,price_open:2479,sl:2475,tp:2485};
const posChart=out.default({symbol:'XAUUSD',positions:[position],market:{},preview:true});
const footer=nodes(posChart).find(n=>n.props.className==='chart-footer');
assert(JSON.stringify(footer.props.children).includes('SL/TP'),'Verified or clearly-marked preview positions must annotate chart levels');
const foreign=out.default({symbol:'XAUUSD',positions:[{...position,symbol:'EURUSD'}],market:{},preview:true});
const foreignFooter=nodes(foreign).find(n=>n.props.className==='chart-footer');
assert(!JSON.stringify(foreignFooter.props.children).includes('SL/TP'),'Positions of another symbol must never appear on the chart');

// Sanitization, chronology, EMA and visible-window regression tests.
const candle=(time,open,close,high,low,tick_volume=20)=>({time,open,close,high,low,tick_volume});
const valid=[
 candle(100,10,11,12,9),candle(200,11,10,11.5,9.5),
 candle(300,10,12,12.2,9),candle(400,12,11,12.1,10.5)
];
const mixed=[valid[2],valid[0],valid[1],valid[2],
 candle(500,10,11,10.5,9),candle(600,-1,-2,-.5,-2.5),valid[3]];
const cleaned=out.cleanChartCandles(mixed);
assert.deepEqual(Array.from(cleaned.map(c=>c.time)),[100,200,300,400],
 'Candles must be chronological, unique and true OHLC; no fabricated correction');
assert.equal(out.chartWindow(cleaned,12,0).bars.at(-1).time,400);
assert.equal(out.chartWindow(cleaned,12,1).bars.at(-1).time,300,'Historical pan must remove latest bar');
assert.equal(out.chartWindow(cleaned,12,300).bars.length,0,'Pan may not invent bars beyond history');
const ema=out.emaSeries(cleaned,9);
assert.equal(ema.length,4);
assert.equal(ema[0],cleaned[0].close);
assert(ema.every(Number.isFinite));
const zoomButton=nodes(chart).find(n=>n.type==='button'&&n.props['aria-label']==='تكبير الشموع');
const resetButton=nodes(chart).find(n=>n.type==='button'&&n.props['aria-label']==='العودة لأحدث شمعة');
assert(typeof zoomButton.props.onClick==='function'&&typeof resetButton.props.onClick==='function');


const previewModule={};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,'mock.ts'),'utf8'),{
 compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}
}).outputText,{exports:previewModule,Math,Date,Number,Object});
const preview=previewModule.generatePreviewCandles(3.8,900);
assert.equal(preview.length,128);
assert.equal(preview.at(-1).close,2479.54,'Fixture must align its explicitly simulated quote');
assert.deepEqual(JSON.stringify(preview),JSON.stringify(previewModule.generatePreviewCandles(3.8,900)),
 'A stable preview must not randomly change while a user is inspecting the design');
assert(preview.every((bar,i)=>bar.time>0&&(i===0||bar.time>preview[i-1].time)&&
 bar.high>=Math.max(bar.open,bar.close)&&bar.low<=Math.min(bar.open,bar.close)&&bar.tick_volume>=0),
 'All fixture candles must be proper OHLC data');
assert(new Set(preview.slice(10,65).map(x=>Math.round(Math.abs(x.close-x.open)*100))).size>15,
 'Preview candles must have varied body shapes, not fixed-period sine candles');
assert(new Set(preview.slice(10,65).map(x=>x.tick_volume)).size>15,
 'Preview volume must vary with the candles');


// Closed-candle and broker precision regression: never render the active
// partial bar as a closed MT5 candle, even if it appears in a bad payload.
assert.equal(out.brokerPriceDigits(0.01),2);
assert.equal(out.brokerPriceDigits(0.001),3);
assert.equal(out.brokerPriceDigits(0.1),1);
assert.equal(out.brokerPriceDigits(null),2);
const nowMs=1_800_000_000_000,nowSec=Math.floor(nowMs/1000);
const realBars=[candle(nowSec-120,2450,2451,2452,2449),
 candle(nowSec-60,2451,2452,2453,2450),
 candle(nowSec,2452,2454,2455,2451)];
assert.equal(out.closedBrokerCandles(realBars,'m1',nowMs).length,2,
 'A source M1 candle still forming must never be called closed');
assert.equal(out.closedBrokerCandles(realBars,'m5',nowMs).length,0,
 'Each timeframe must apply its own close horizon');
assert.equal(out.lastClosedAt(realBars.slice(0,1),'m1'),
 new Date((nowSec-60)*1000).toISOString().slice(0,16).replace('T',' ')+' UTC');
assert.equal(out.lastClosedAt([],'m1'),null);
const nowClosed=out.closedBrokerCandles(realBars,'m1',nowMs);
assert.equal(nowClosed.at(-1).time,nowSec-60);

console.log('PASS: all seven chart timeframes select the real chart state');
