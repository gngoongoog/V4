// Exercises the real TradingSettingsOperations state and save callback without a browser.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const ts=require('typescript');
const root=path.resolve(__dirname,'..');
const source=fs.readFileSync(path.join(root,'src/operations.tsx'),'utf8');
const output=ts.transpileModule(source,{fileName:'operations.tsx',compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX},reportDiagnostics:true});
assert.deepEqual((output.diagnostics||[]).filter(x=>x.category===ts.DiagnosticCategory.Error),[]);
const jsx=(type,props)=>({type,props:props||{}});
const state=[];
const dependencies=[];
let index=0,effects=[];
const hooks={
 useState(initial){
  const i=index++;
  if(!(i in state))state[i]=typeof initial==='function'?initial():initial;
  return [state[i],next=>{state[i]=typeof next==='function'?next(state[i]):next;}];
 },
 useRef(initial){
  const i=index++;
  if(!(i in state))state[i]={current:initial};
  return state[i];
 },
 useEffect(fn,deps){
  const i=index++;
  if(!dependencies[i]||deps.some((v,n)=>!Object.is(v,dependencies[i][n]))){
   dependencies[i]=deps;effects.push(fn);
  }
 }
};
const exportsObj={};
vm.runInNewContext(output.outputText,{exports:exportsObj,require(id){
 if(id==='react')return hooks;
 if(id==='react/jsx-runtime')return {jsx,jsxs:jsx};
 if(id==='lucide-react')return Object.fromEntries(['Archive','CalendarClock','Download','Play','Square','ShieldAlert'].map(k=>[k,k]));
 throw Error('Unknown module '+id);
},Date,Number,String,JSON,console});
function nodes(node,out=[]){
 if(!node||typeof node!=='object')return out;
 if(Array.isArray(node)){node.forEach(n=>nodes(n,out));return out;}
 out.push(node);nodes(node.props?.children,out);return out;
}
let sourceData={config:{
 trading:{mode:'sniper',priority_enabled:true,sniper_active:true,fallback_mode:'expert',
 sniper_style:'auto',sniper_brain:'classic',sniper_target_mode:'fixed',
 sniper_target_points:50,sniper_stop_points:80,pip_points:10,sniper_pip_price:0.1,
 sniper_cooldown_seconds:0,sniper_max_hold_seconds:120,sniper_min_target_pips:2,
 sniper_max_target_pips:10,max_cost_fraction:0.45,demo_flexible:true,adaptive_exits:true},
 risk:{slippage_ticks:2}},
 commission_editor:{amount:7,basis:'round_trip',
 account:{login:123,server:'Exness-MT5Trial15',currency:'USD',symbol:'XAUUSDr'}}};
let calls=0,latest,shouldReject=true;
const apply=async input=>{
 calls++;latest=input;
 if(shouldReject)throw Error('Mock Core rejected save');
};
function render(){
 index=0;const view=exportsObj.TradingSettingsOperations({source:sourceData,disabled:false,armed:false,apply});
 const queued=effects;effects=[];queued.forEach(f=>f());
 return view;
}
function findInput(view,label){
 const node=nodes(view).find(n=>n.props?.['aria-label']===label);
 assert(node,'Missing control '+label);return node;
}
function saveButton(view){
 const node=nodes(view).find(n=>n.type==='button'&&typeof n.props.onClick==='function'
 &&String(n.props.children).includes('حفظ الوضع'));
 assert(node,'Missing save button');return node;
}
const flush=async()=>{await new Promise(resolve=>setImmediate(resolve));};
(async()=>{
 render();
 let view=render();
 assert.equal(findInput(view,'sniper fixed target pips').props.value,'5');
 findInput(view,'sniper fixed target pips').props.onChange({target:{value:'7.5'}});
 view=render();
 assert.equal(findInput(view,'sniper fixed target pips').props.value,'7.5');
 findInput(view,'sniper points per pip').props.onChange({target:{value:'20'}});
 view=render();
 assert.equal(findInput(view,'sniper fixed target pips').props.value,'7.5','Pip conversion must retain user-selected target');
 findInput(view,'sniper cooldown seconds').props.onChange({target:{value:''}});
 view=render();
 saveButton(view).props.onClick();await flush();
 assert.equal(calls,0,'Empty cooldown must not convert silently to zero');
 view=render();
 assert.equal(findInput(view,'sniper cooldown seconds').props.value,'');
 assert(nodes(view).some(n=>n.props?.role==='alert'),'Invalid field must report error');
 findInput(view,'sniper cooldown seconds').props.onChange({target:{value:'3'}});
 view=render();
 findInput(view,'commission per lot').props.onChange({target:{value:'0'}});
 view=render();
 saveButton(view).props.onClick();await flush();
 assert.equal(calls,1);
 assert.equal(latest.trading.sniper_target_points,150);
 assert.equal(latest.trading.pip_points,20);
 assert.equal(typeof latest.trading.sniper_target_points,'number','Core expects JSON numbers, not numeric strings');
 assert.equal(latest.trading.sniper_cooldown_seconds,3);
 assert.equal(latest.commission_per_lot,0);
 assert.equal(latest.account.symbol,'XAUUSDr');
 view=render();
 assert.equal(findInput(view,'sniper cooldown seconds').props.value,'3','Rejected save must retain unsaved edits');
 assert.equal(findInput(view,'sniper fixed target pips').props.value,'7.5');
 assert(nodes(view).some(n=>n.props?.role==='alert'),'Rejected Core save must be surfaced');
 shouldReject=false;
 saveButton(view).props.onClick();await flush();
 assert.equal(calls,2);
 // Parent re-reads verified values from Core; UI must display those, not an old draft.
 sourceData={...sourceData,config:{...sourceData.config,trading:{...latest.trading}} ,
  commission_editor:{...sourceData.commission_editor,amount:latest.commission_per_lot}};
 render();view=render();
 assert.equal(findInput(view,'sniper points per pip').props.value,'20');
 assert.equal(Number(findInput(view,'sniper fixed target pips').props.value),7.5);
 assert.equal(findInput(view,'sniper cooldown seconds').props.value,'3');
 assert.equal(findInput(view,'commission per lot').props.value,'0');
 // A background Core revision, account or risk change must not erase edits or
 // allow an older draft to overwrite settings the operator has not reviewed.
 findInput(view,'sniper cooldown seconds').props.onChange({target:{value:'9'}});
 view=render();
 sourceData={...sourceData,config:{...sourceData.config,risk:{slippage_ticks:3}}};
 view=render();
 assert.equal(findInput(view,'sniper cooldown seconds').props.value,'9','Refresh must retain the draft');
 saveButton(view).props.onClick();await flush();
 assert.equal(calls,2,'Stale draft must not call Core');
 assert(nodes(render()).some(n=>n.props?.role==='alert'),'Stale draft must explain the conflict');
 const reset=nodes(render()).find(n=>n.type==='button'&&String(n.props.children).includes('التراجع'));
 assert(reset,'Missing reset button');reset.props.onClick();
 view=render();
 assert.equal(findInput(view,'sniper cooldown seconds').props.value,'3','Reset must show the current Core settings');
 findInput(view,'sniper cooldown seconds').props.onChange({target:{value:'9'}});
 view=render();saveButton(view).props.onClick();await flush();
 assert.equal(calls,3);assert.equal(latest.slippage_ticks,3);
 assert.equal(latest.trading.sniper_cooldown_seconds,9);
 console.log('PASS: edits, pip geometry, rejection, reread and stale draft conflict');
})().catch(e=>{console.error(e);process.exitCode=1});
