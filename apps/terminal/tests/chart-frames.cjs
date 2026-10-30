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
console.log('PASS: all seven chart timeframes select the real chart state');
