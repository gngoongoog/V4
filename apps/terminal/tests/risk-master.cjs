const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require('typescript');
const root=path.resolve(__dirname,'../src');
const componentSource=fs.readFileSync(path.join(root,'risk-master-operations.tsx'),'utf8');
const output=ts.transpileModule(componentSource,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}}).outputText;
const exported={},jsx=(type,props)=>({type,props:props||{}});
vm.runInNewContext(output,{exports:exported,require(id){
 if(id==='react/jsx-runtime')return {jsx,jsxs:jsx};
 if(id==='react')return {useState:value=>[value,()=>{}]};
 if(id==='lucide-react')return {ShieldCheck:'ShieldCheck'};
 throw Error('Unexpected module '+id);
},JSON});
function nodes(n,out=[]){if(!n||typeof n!=='object')return out;if(Array.isArray(n)){n.forEach(v=>nodes(v,out));return out}out.push(n);nodes(n.props?.children,out);return out}
const t={connected:true,demo:true,account_trade_mode:0,login:17,server:'Demo',symbol:'XAUUSD',currency:'USD',balance:50,equity:48};
const status={gateway_fresh:true,telemetry:t};
const source={account_matches:true,context:JSON.stringify([t.server,t.login,t.symbol,t.currency]),settings:{target:'demo',login:17,server:'Demo',currency:'USD'},risk_master:{enabled:true,scope:'bound_broker_account_and_mode'}};
const changed=[];
const render=(s=status,v=source,armed=false)=>exported.RiskMasterOperations({status:s,source:v,disabled:false,armed,onSet:async (enabled,recovery)=>changed.push([enabled,recovery])});
let button=nodes(render()).find(n=>n.type==='button');assert(!button.props.disabled);button.props.onClick();
let selected=changed.pop();assert.equal(selected[0],false);assert.equal(selected[1].basis,'balance');assert.equal(selected[1].threshold,500);
button=nodes(render(status,{...source,risk_master:{...source.risk_master,enabled:false}})).find(n=>n.type==='button');
assert(!button.props.disabled);button.props.onClick();selected=changed.pop();assert.equal(selected[0],true);assert.equal(selected[1],null);
assert(nodes(render({...status,telemetry:{...t,balance:500}})).find(n=>n.type==='button').props.disabled,'Cannot suspend risk at a recovery threshold already reached');
for(const [s,v,armed] of [[{...status,gateway_fresh:false},source,false],[status,{...source,context:'old account'},false],[status,{...source,settings:{...source.settings,target:'live'}},false],[status,{...source,risk_master:{}},false],[status,source,true]]){
 assert(nodes(render(s,v,armed)).find(n=>n.type==='button').props.disabled,'Unverified account, mode, state or running engine must disable risk changes');
}
const main=fs.readFileSync(path.join(root,'terminal.tsx'),'utf8');
assert(main.includes('confirmLiveLogin!==String(t.login)')&&main.includes('updateRiskMaster(context,enabled,confirmLiveLogin,recovery)'));
console.log('PASS: risk master UI is account/mode scoped, requires STOP and dispatches explicit on/off state');
