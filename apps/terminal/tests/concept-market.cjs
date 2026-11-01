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
  if(id==='react')return {useState:value=>[value,()=>{}]};
  if(id==='lucide-react')return Object.fromEntries(['ArrowDown','ArrowUp','BriefcaseBusiness','ChartCandlestick','Clock','RefreshCw','Settings','ShieldCheck','Target','Wallet','Zap'].map(k=>[k,k]));
  if(id==='./chart')return {default:'TradingChart'};
  if(id==='./gold-symbol')return {GoldSymbol:'GoldSymbol'};
  if(id==='./format')return fmt;
  throw Error('unexpected import '+id);
 }});
function nodes(n,out=[]){if(!n||typeof n!=='object')return out;if(Array.isArray(n)){n.forEach(v=>nodes(v,out));return out}out.push(n);nodes(n.props?.children,out);return out}
let actions=[];
const account={connected:true,demo:true,symbol:'XAUUSD',currency:'USD',balance:100,margin_free:80,
 bid:2000.1,ask:2000.3,trade_allowed:true,trade_expert:true,positions:[]};
const props={account,status:{gateway_fresh:true,requested_run:false},accountMatches:true,quoteFresh:true,preview:false,busy:false,native:true,
 frame:'m15',onFrameChange:()=>{},theme:'light',open:p=>actions.push(p),onTradingSettings:()=>actions.push('settings'),
 onStart:()=>actions.push('start'),onStop:()=>actions.push('stop'),onClose:profit=>actions.push(profit?'close-profit':'close-owned'),onCloseTicket:ticket=>actions.push('ticket-'+ticket),onManual:input=>actions.push(input)};
let tree=moduleExports.ConceptMarketDesk(props),all=nodes(tree);
assert(all.some(n=>n.props?.className==='concept-led on'),'Fresh live quotes have a green indicator');
const start=all.find(n=>n.type==='button'&&n.props.className==='action-btn start');
const stop=all.find(n=>n.type==='button'&&n.props.className==='action-btn stop');
assert.equal(start.props.disabled,false);assert.equal(stop.props.disabled,false);
start.props.onClick();stop.props.onClick();assert.deepEqual(actions,['start','stop']);
const unsupported=all.filter(n=>n.type==='button'&&/غير مدعوم|أوامر .* غير متاحة/.test(n.props.title||''));
assert.equal(unsupported.length,0,'Unsupported broker order types must not appear as fake interactive buttons');
assert(fs.readFileSync(path.join(root,'concept-market.tsx'),'utf8').includes('إنشاء أوامر شراء/بيع معلقة غير مدعوم'),
 'Unavailable pending-order creation must be explained explicitly rather than simulated');
assert.equal(moduleExports.manualDraft(1,'','','',2000,2000.1),null);
assert.equal(moduleExports.manualDraft(1,'0.1','2001','2002',2000,2000.1),null);
const validBuy=moduleExports.manualDraft(1,'0.1','1999','2002',2000,2000.1);
assert.equal(validBuy.reference,2000.1);assert.equal(validBuy.stop,1999);assert.equal(validBuy.lots,0.1);
const validSell=moduleExports.manualDraft(-1,'0.1','2001','1998',2000,2000.1);
assert.equal(validSell.reference,2000);assert.equal(validSell.target,1998);
const prices=all.find(n=>n.props?.className==='concept-prices');
assert(nodes(prices).some(n=>n.type==='strong'&&n.props.children==='2,000.10'));
tree=moduleExports.ConceptMarketDesk({...props,account:null,status:{gateway_fresh:false,requested_run:false},quoteFresh:false});
all=nodes(tree);assert.equal(all.find(n=>n.type==='button'&&n.props.className==='action-btn start').props.disabled,true);
assert(nodes(all.find(n=>n.props?.className==='concept-prices')).filter(n=>n.type==='strong').every(n=>n.props.children==='—'));
tree=moduleExports.ConceptMarketDesk({...props,quoteFresh:false});all=nodes(tree);
assert(nodes(all.find(n=>n.props?.className==='concept-prices')).filter(n=>n.type==='strong').every(n=>n.props.children==='—'),'Stale broker quotes must not appear as live prices');
assert(!all.some(n=>n.props?.className==='concept-led on'),'An account connection alone must not make stale market data green');
tree=moduleExports.ConceptMarketDesk({...props,preview:true});all=nodes(tree);
assert(!all.some(n=>n.props?.className==='concept-led on'),'Synthetic preview must not indicate a live market');
const positions=[
 {ticket:101,magic:410600,symbol:'XAUUSD',side:'BUY',volume:.1,profit:4},
 {ticket:102,magic:410617,symbol:'XAUUSD',side:'SELL',volume:.2,profit:-1},
 {ticket:103,magic:999,symbol:'XAUUSD',side:'BUY',volume:.1,profit:9},
 {ticket:104,magic:410600,symbol:'EURUSD',side:'BUY',volume:.1,profit:9},
 {ticket:105,magic:410616,symbol:'XAUUSD',side:'BUY',volume:.1,profit:9},
];
assert.deepEqual(Array.from(moduleExports.ownedCloseTickets(positions,'XAUUSD',false)),[101,102]);
assert.deepEqual(Array.from(moduleExports.ownedCloseTickets(positions,'XAUUSD',true)),[101]);
assert.deepEqual(Array.from(moduleExports.ownedCloseTickets([positions[0],positions[0]],'XAUUSD',false)),[],'duplicate broker tickets must fail closed');
tree=moduleExports.ConceptMarketDesk({...props,account:{...account,positions}});all=nodes(tree);
const close=all.find(n=>n.type==='button'&&n.props.children==='إغلاق مراكز البرنامج للرمز');
const profit=all.find(n=>n.type==='button'&&n.props.children==='إغلاق الرابحة للرمز');
assert(!close.props.disabled&&!profit.props.disabled);close.props.onClick();profit.props.onClick();
assert.deepEqual(actions.slice(-2),['close-owned','close-profit']);
const selected=all.find(n=>n.type==='button'&&n.props['aria-label']==='إغلاق المركز 101');
const foreign=all.find(n=>n.type==='button'&&n.props['aria-label']==='إغلاق المركز 103');
assert(!selected.props.disabled);assert(foreign.props.disabled,'Foreign positions cannot be closed by the owned-position action');
selected.props.onClick();assert.equal(actions.at(-1),'ticket-101','The row action must request exactly the clicked ticket');
for(const change of [{preview:true},{quoteFresh:false},{accountMatches:false},{status:{gateway_fresh:true,requested_run:true}},{native:false}]){
 const view=moduleExports.ConceptMarketDesk({...props,account:{...account,positions},...change});
 assert(nodes(view).find(n=>n.type==='button'&&n.props.children==='إغلاق مراكز البرنامج للرمز').props.disabled,'close must fail closed in preview, stale quote, running or non-native state');
 assert(nodes(view).find(n=>n.type==='button'&&n.props['aria-label']==='إغلاق المركز 101').props.disabled,'individual close must obey the same execution guard');
 assert(nodes(view).find(n=>n.type==='button'&&n.props.className==='action-btn start').props.disabled,'START must not appear ready when scope, quotes, runtime or preview is untrusted');
}
const wrongScope=moduleExports.ConceptMarketDesk({...props,account:{...account,positions},accountMatches:false});
assert(nodes(wrongScope).some(n=>n.type==='button'&&String(Array.isArray(n.props.children)?n.props.children.join(''):n.props.children)==='تنفيذ شراء بسعر السوق'&&n.props.disabled===true),'Manual entry must be disabled when the account scope is not reconciled');

assert.equal(moduleExports.steppedDraftValue('0.01',0.01,1,0.01,0.05),'0.02');
assert.equal(moduleExports.steppedDraftValue('0.05',0.01,1,0.01,0.05),'0.05','Broker volume maximum must cap the draft');
assert.equal(moduleExports.steppedDraftValue('0.01',0.01,-1,0.01,0.05),'0.01','Broker volume minimum must cap the draft');
assert.equal(moduleExports.steppedDraftValue('2479.50',0.01,1),'2479.51');
assert.equal(moduleExports.steppedDraftValue('',0.01,1),'','Do not invent an SL/TP when the user has not entered one');
const steppers=nodes(moduleExports.ConceptMarketDesk({...props,account:{...account,volume_step:0.01,volume_min:0.01,volume_max:2,leverage:500,point:0.01}}));
assert(steppers.some(n=>n.type==='button'&&n.props['aria-label']==='زيادة اللوت'&&typeof n.props.onClick==='function'));
assert(steppers.some(n=>n.type==='button'&&n.props.title?.includes('الرافعة يحددها الوسيط')&&typeof n.props.onClick==='function'));
console.log('PASS: concept market scopes requested closes to exact owned tickets, protects preview/STOP/quotes, and never renders unsupported pending-order buttons');
