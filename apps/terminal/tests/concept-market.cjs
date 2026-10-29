// Verify that new visual controls preserve the existing execution boundary.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require('typescript');
const root=path.resolve(__dirname,'../src');
const compile=file=>ts.transpileModule(fs.readFileSync(path.join(root,file),'utf8'),{fileName:file,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}}).outputText;
const fmt={};vm.runInNewContext(compile('format.ts'),{exports:fmt,Number,String,Math});
const moduleExports={},jsx=(type,props)=>({type,props:props||{}});
vm.runInNewContext(compile('concept-market.tsx'),{exports:moduleExports,Number,String,Math,
 require(id){
  if(id==='react/jsx-runtime')return {jsx,jsxs:jsx};
  if(id==='lucide-react')return Object.fromEntries(['ArrowDown','ArrowUp','BriefcaseBusiness','ChartCandlestick','Clock','RefreshCw','Settings','ShieldCheck','Target','Wallet','Zap'].map(k=>[k,k]));
  if(id==='./chart')return {default:'TradingChart'};
  if(id==='./gold-symbol')return {GoldSymbol:'GoldSymbol'};
  if(id==='./format')return fmt;
  throw Error('unexpected import '+id);
 }});
function nodes(n,out=[]){if(!n||typeof n!=='object')return out;if(Array.isArray(n)){n.forEach(v=>nodes(v,out));return out}out.push(n);nodes(n.props?.children,out);return out}
let actions=[];
const account={connected:true,demo:true,symbol:'XAUUSD',currency:'USD',balance:100,margin_free:80,
 bid:2000.1,ask:2000.3,positions:[]};
const props={account,status:{gateway_fresh:true,requested_run:false},quoteFresh:true,preview:false,busy:false,native:true,
 frame:'m15',onFrameChange:()=>{},theme:'light',open:p=>actions.push(p),onTradingSettings:()=>actions.push('settings'),
 onStart:()=>actions.push('start'),onStop:()=>actions.push('stop')};
let tree=moduleExports.ConceptMarketDesk(props),all=nodes(tree);
const start=all.find(n=>n.type==='button'&&n.props.className==='action-btn start');
const stop=all.find(n=>n.type==='button'&&n.props.className==='action-btn stop');
assert.equal(start.props.disabled,false);assert.equal(stop.props.disabled,false);
start.props.onClick();stop.props.onClick();assert.deepEqual(actions,['start','stop']);
const unsupported=all.filter(n=>n.type==='button'&&/غير مدعوم|أوامر .* غير متاحة/.test(n.props.title||''));
assert.equal(unsupported.length,7);
for(const button of unsupported)
 assert.equal(button.props.disabled,true,'Unsupported manual orders must never act as broker operations');
const prices=all.find(n=>n.props?.className==='concept-prices');
assert(nodes(prices).some(n=>n.type==='strong'&&n.props.children==='2,000.10'));
tree=moduleExports.ConceptMarketDesk({...props,account:null,status:{gateway_fresh:false,requested_run:false},quoteFresh:false});
all=nodes(tree);assert.equal(all.find(n=>n.type==='button'&&n.props.className==='action-btn start').props.disabled,true);
assert(nodes(all.find(n=>n.props?.className==='concept-prices')).filter(n=>n.type==='strong').every(n=>n.props.children==='—'));
tree=moduleExports.ConceptMarketDesk({...props,quoteFresh:false});all=nodes(tree);
assert(nodes(all.find(n=>n.props?.className==='concept-prices')).filter(n=>n.type==='strong').every(n=>n.props.children==='—'),'Stale broker quotes must not appear as live prices');
console.log('PASS: concept market uses fresh prices, dispatches existing START/STOP, and does not enable unsupported manual orders');
