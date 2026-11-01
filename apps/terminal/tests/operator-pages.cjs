// Render the real operator page components with TSX transpilation, without a broker.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const ts=require('typescript');
const source=fs.readFileSync(path.resolve(__dirname,'../src/operator-pages.tsx'),'utf8');
const compiled=ts.transpileModule(source,{fileName:'operator-pages.tsx',compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX},reportDiagnostics:true});
assert(!compiled.diagnostics?.some(e=>e.category===ts.DiagnosticCategory.Error),'new operator workspaces TSX must transpile');
const api={},jsx=(type,props)=>({type,props:props||{}});
const hooks={useState:initial=>[initial,()=>{}],useMemo:calculate=>calculate()};
vm.runInNewContext(compiled.outputText,{exports:api,require:name=>{
 if(name==='react')return hooks;
 if(name==='react/jsx-runtime')return {jsx,jsxs:jsx};
 if(name==='lucide-react')return new Proxy({}, {get:(_,p)=>p});
 if(name.endsWith('.css'))return {};
 throw Error('unexpected import '+name);
},URL,Number,JSON,Date,console});
function collect(n,out=[]){if(n===null||n===undefined)return out;if(Array.isArray(n)){n.forEach(x=>collect(x,out));return out}
 if(typeof n==='object'){out.push(n);collect(n.props?.children,out)}return out}
function rawText(n){if(typeof n==='string'||typeof n==='number')return String(n);if(Array.isArray(n))return n.map(rawText).join(' ');return n&&typeof n==='object'?rawText(n.props?.children):''}
(async()=>{
 const sources=[{id:'fed',name:'Federal Reserve',last_success_ms:1000,stale:false,items:2}];
 const src={enabled:true,offline:false,refreshing:false,sources,macro:{fresh_sources:1,recent_60m:2,current:true,state:'recent'},items:[
  {title:'Genuine policy note',topic:'rates',url:'https://www.federalreserve.gov/newsevents/',source:'Federal Reserve',source_stale:false,published_ms:1700000000000},
  {title:'Unsafe external page',topic:'phish',url:'https://attacker.example/steal',source:'Federal Reserve',source_stale:false,published_ms:1700000000001}
 ]};
 const actions=[];
 const page=api.NewsOperatorWorkspace({source:src,error:'',preview:false,disabled:false,onRefresh:()=>{},onAction:async action=>actions.push(action)});
 const links=collect(page).filter(x=>x.type==='a');
 assert.equal(links.length,1,'only official allowlisted article links can be clickable');
 assert.equal(links[0].props.href,'https://www.federalreserve.gov/newsevents/');
 assert.equal(links[0].props.rel,'noopener noreferrer');
 const update=collect(page).find(x=>x.type==='button'&&rawText(x).includes('جلب الأخبار الآن'));
 assert(update&&!update.props.disabled,'fresh enabled source must permit explicit refresh');
 update.props.onClick();await Promise.resolve();assert.deepEqual(actions,['refresh']);
 const disabled=api.NewsOperatorWorkspace({source:{...src,enabled:false},error:'',preview:false,disabled:false,onRefresh:()=>{},onAction:async action=>actions.push(action)});
 assert(collect(disabled).find(x=>x.type==='button'&&rawText(x).includes('جلب الأخبار الآن')).props.disabled,'disabled feed cannot refresh');
 const on=[];
 const studio=api.CoreReadoutWorkspace({page:'studio',source:{connected:true,learning_frozen:false,auto:false,queued:0,frames:[{timeframe:'M5',bars:20,live:true}],reports:[]},preview:false,error:'',disabled:false,onRefresh:()=>{},onNavigate:()=>{},onStudioAction:async (action,tf)=>{on.push([action,tf]);return {queued:1}}});
 const run=collect(studio).find(x=>x.type==='button'&&rawText(x).includes('تشغيل تجربة بحثية'));
 assert(run&&!run.props.disabled,'live researched frame allows queued research');
 run.props.onClick();await Promise.resolve();assert.equal(on[0][0],'run');assert.equal(on[0][1],'ALL');
 const frozen=api.CoreReadoutWorkspace({page:'studio',source:{connected:true,learning_frozen:true,auto:false,frames:[{timeframe:'M5',bars:20}]},preview:false,error:'',disabled:false,onRefresh:()=>{},onNavigate:()=>{},onStudioAction:async()=>{throw Error('must never run frozen research')}});
 assert(collect(frozen).find(x=>x.type==='button'&&rawText(x).includes('تشغيل تجربة بحثية')).props.disabled,'frozen learning blocks launch');
 const positions=[{ticket:1,magic:410600,side:'BUY',symbol:'XAUUSD',volume:.01,price_open:2500,price_current:2501,profit:1,sl:2490,tp:2510},{ticket:2,magic:0,side:'SELL',symbol:'EURUSD',volume:.03,price_open:1.1,price_current:1.11,profit:-3,sl:1.2,tp:1.0}];
 const pos=api.PositionsOperatorWorkspace({positions,currency:'USD',accountAvailable:true,onTrading:()=>{}});
 assert.equal(collect(pos).filter(x=>x.type==='tr').length,3,'header and two actual MT5 positions displayed');
 const disconnected=api.PositionsOperatorWorkspace({positions:[],currency:'USD',accountAvailable:false,onTrading:()=>{}});
 assert.equal(collect(disconnected).filter(x=>x.type==='table').length,0,'offline must not claim MT5 has zero positions');
 for(const id of ['analysis','shadow','events','correlation']){
  const node=api.CoreReadoutWorkspace({page:id,source:id==='correlation'?{saved_backtests:2,saved_reports:1,research_only:true}:id==='analysis'?{setups:[{setup_id:'A',first_seen_ms:1700000000000}]}:id==='shadow'?{decisions:[{id:1,decision:{analysis:{action:'BUY'}}}]}:{events:[{id:1,kind:'CORE_READY',detail:{ok:true}}]},preview:false,error:'',disabled:false,onRefresh:()=>{},onNavigate:()=>{},onStudioAction:async()=>{}});
  assert(collect(node).length>5,id+' must have a real-purpose operator view');
 }
 console.log('PASS: operator news, Studio, event/research pages and MT5 positions render without invented broker state');
})().catch(e=>{console.error(e);process.exitCode=1});
