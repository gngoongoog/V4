const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require('typescript'),root=path.resolve(__dirname,'../src');
const compile=file=>ts.transpileModule(fs.readFileSync(path.join(root,file),'utf8'),{fileName:file,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}}).outputText;
const directory={};vm.runInNewContext(compile('brain-catalog.ts'),{exports:directory});
const exported={},jsx=(type,props)=>({type,props:props||{}});let choice='';
vm.runInNewContext(compile('solo-operations.tsx'),{exports:exported,require(id){
 if(id==='react')return {useState:()=>[choice,()=>{}]};
 if(id==='react/jsx-runtime')return {jsx,jsxs:jsx};
 if(id==='./brain-catalog')return directory;
 throw Error('Unexpected module '+id);
},JSON});
function nodes(n,out=[]){if(!n||typeof n!=='object')return out;if(Array.isArray(n)){n.forEach(v=>nodes(v,out));return out}out.push(n);nodes(n.props?.children,out);return out}
const t={connected:true,demo:true,account_trade_mode:0,login:17,server:'Demo',symbol:'XAUUSD',currency:'USD'};
const status={gateway_fresh:true,telemetry:t};
const source={account_matches:true,controls_available:true,context:JSON.stringify([t.server,t.login,t.symbol,t.currency]),trading_mode:'expert',selected_owner:'system',settings:{target:'demo',login:17,server:'Demo',currency:'USD'},brains:[{config:{id:'system'}},{config:{id:'gann'}},{config:{id:'trend'}}]};
const selected=[];
const render=(s=status,v=source,armed=false)=>exported.SoloOperations({status:s,source:v,disabled:false,armed,onSet:async brain=>selected.push(brain)});
let buttons=nodes(render()).filter(n=>n.type==='button');assert(!buttons[0].props.disabled&&buttons[1].props.disabled);
buttons[0].props.onClick();assert.equal(selected.pop(),'gann');
choice='trend';buttons=nodes(render(status,{...source,selected_owner:'trend'})).filter(n=>n.type==='button');
assert(buttons[0].props.disabled&&!buttons[1].props.disabled);buttons[1].props.onClick();assert.equal(selected.pop(),'system');
for(const [s,v,armed] of [[{...status,gateway_fresh:false},source,false],[status,{...source,context:'old'},false],[status,{...source,settings:{...source.settings,target:'live'}},false],[status,{...source,selected_owner:'unknown'},false],[status,source,true]]){
 assert(nodes(render(s,v,armed)).filter(n=>n.type==='button').every(b=>b.props.disabled),'Solo must fail closed on stale account, mode, owner or running engine');
}
console.log('PASS: solo UI selects one scoped brain or returns to coordinator, and requires STOP');
