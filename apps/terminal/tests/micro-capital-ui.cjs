const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),ts=require('typescript');
const root=path.resolve(__dirname,'../src');
function transpile(name,jsx=false){
 const source=fs.readFileSync(path.join(root,name),'utf8');
 return ts.transpileModule(source,{fileName:name,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:jsx?ts.JsxEmit.ReactJSX:undefined}}).outputText;
}
const readiness={};vm.runInNewContext(transpile('micro-readiness.ts'),{exports:readiness});
const helper={};vm.runInNewContext(transpile('micro-capital.ts'),{exports:helper,require:id=>{
 if(id==='./micro-readiness')return readiness;
 throw Error('Unexpected helper dependency '+id);
}});
const jsx=(type,props)=>({type,props:props||{}});
const hooks={useState:initial=>[initial,()=>{}],useEffect:()=>{}};
const component={};
vm.runInNewContext(transpile('micro-capital-operations.tsx',true),{
 exports:component,require:id=>{
  if(id==='react')return hooks;
  if(id==='react/jsx-runtime')return {jsx,jsxs:jsx};
  if(id==='lucide-react')return {ShieldAlert:'ShieldAlert',Wallet:'Wallet',SlidersHorizontal:'SlidersHorizontal'};
  if(id==='./micro-capital')return helper;
  if(id==='./micro-readiness')return readiness;
  throw Error('Unknown dependency '+id);
 },console,Date,Number,String,JSON,window:{confirm:()=>true}
});
function nodes(node,out=[]){
 if(!node||typeof node!=='object')return out;
 if(Array.isArray(node)){node.forEach(n=>nodes(n,out));return out;}
 out.push(node);nodes(node.props?.children,out);return out;
}
const source={enabled:true,limits:{max_positions:3,max_lots:0.3,max_risk_percent:2}};
const status={gateway_fresh:true,telemetry:{connected:true,demo:false,account_trade_mode:2,login:12345,server:'REAL',currency:'USD',equity:50,margin_free:42,minimum_lot_margin:11,volume_min:.01,leverage:500}};
const risk={risk_percent:.25,micro_real_temporary:true};
const now=Date.now(),telemetry=status.telemetry;
const identity={login:telemetry.login,server:telemetry.server,currency:telemetry.currency,mode:telemetry.account_trade_mode};
const monitor={observed_ms:now,state:'ready',enabled:true,reason:'ready',account:identity,
 micro_real:{schema:'p1e-micro-recovery-v1',observed_ms:now,read_only:true,broker_execution_authorized:false,
 account:identity,state:'candidate_only',candidate_only:true,configured:true,recovered_persisted:false,
 current_milestone_reached:false,cap_percent:10,recovery_equity:500}};
(async()=>{
 let calls=[];
 const props={status,risk,monitor,portfolio:source,armed:false,disabled:false,onSavePortfolio:async v=>{calls.push(JSON.parse(JSON.stringify(v)));}};
 const view=component.MicroCapitalOperations(props);
 const all=nodes(view),form=all.find(n=>n.type==='form');
 assert(form,'portfolio controls form renders from actual Core source');
 const save=all.find(n=>n.type==='button'&&n.props.type==='submit');
 assert(save&&!save.props.disabled);
 for(const label of ['portfolio max positions','portfolio max lots','portfolio max risk percent']){
  assert(all.find(n=>n.type==='input'&&n.props['aria-label']===label),'missing editable '+label);
 }
 const scenario=all.find(n=>n.type==='input'&&n.props['aria-label']==='indicative scenario leverage');
 assert(scenario&&scenario.props.value==='500','scenario is separate from actual broker leverage');
 form.props.onSubmit({preventDefault(){}});await Promise.resolve();await Promise.resolve();
 assert.equal(calls.length,1);
 assert.deepEqual(calls[0],{enabled:true,max_positions:3,max_lots:0.3,max_risk_percent:2});
 const armed=component.MicroCapitalOperations({...props,armed:true});
 const armedNodes=nodes(armed),armedForm=armedNodes.find(n=>n.type==='form');
 assert(armedNodes.find(n=>n.type==='button'&&n.props.type==='submit').props.disabled);
 armedForm.props.onSubmit({preventDefault(){}});await Promise.resolve();
 assert.equal(calls.length,1,'START state must not dispatch portfolio edits');
 const offline=component.MicroCapitalOperations({...props,status:null,monitor:null,portfolio:null});
 assert(!nodes(offline).some(n=>n.type==='form'),'no editable fake portfolio without Core source');
 const prior=hooks.useState;let counter=0;
 hooks.useState=initial=>{counter++;return [counter===2?{enabled:true,max_positions:3,max_lots:0.3,max_risk_percent:101}:initial,()=>{}]};
 const invalid=component.MicroCapitalOperations(props);
 nodes(invalid).find(n=>n.type==='form').props.onSubmit({preventDefault(){}});
 await Promise.resolve();await Promise.resolve();
 assert.equal(calls.length,1,'invalid risk policy is never sent to Core');
 hooks.useState=prior;
 console.log('PASS: Micro REAL panel renders verified MT5/portfolio facts; submit calls Core only in STOPPED mode; missing or invalid policy rejected');
})().catch(error=>{console.error(error);process.exitCode=1});
