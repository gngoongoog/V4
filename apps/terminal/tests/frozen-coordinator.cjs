const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),ts=require('typescript');
const dir=path.resolve(__dirname,'../src');
const read=file=>fs.readFileSync(path.join(dir,file),'utf8');
const js=ts.transpileModule(read('frozen-coordinator-operations.tsx'),{
 fileName:'frozen-coordinator-operations.tsx',
 compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,
  jsx:ts.JsxEmit.ReactJSX}
}).outputText;
const component={},jsx=(type,props)=>({type,props:props||{}});
vm.runInNewContext(js,{exports:component,require:name=>{
 if(name==='react/jsx-runtime')return {jsx,jsxs:jsx};
 throw Error('Unexpected dependency '+name);
},Number,Object,String,JSON,Date});
const {frozenExpertEligible,FrozenCoordinatorOperations}=component;
const telemetry={connected:true,demo:true,account_trade_mode:0,login:17,
 server:'Demo',currency:'USD',symbol:'XAUUSD'};
const status={gateway_fresh:true,telemetry};
const ctx=JSON.stringify(['Demo',17,'XAUUSD','USD']);
assert.equal(frozenExpertEligible(status,'expert',ctx,false),true);
for(const [label,override] of [
 ['REAL',{telemetry:{...telemetry,demo:false,account_trade_mode:2}}],
 ['stale',{gateway_fresh:false}],
 ['disconnected',{telemetry:{...telemetry,connected:false}}],
 ['different MT5 login',{telemetry:{...telemetry,login:18}}],
 ['different symbol',{telemetry:{...telemetry,symbol:'EURUSD'}}],
 ['unknown mode',{telemetry:{...telemetry,account_trade_mode:null}}],
]){
 const data={...status,...override};
 assert.equal(frozenExpertEligible(data,'expert',ctx,false),false,label);
}
assert.equal(frozenExpertEligible(status,'sniper',ctx,false),false);
assert.equal(frozenExpertEligible(status,'scalping',ctx,false),false);
assert.equal(frozenExpertEligible(status,'swing',ctx,false),false);
assert.equal(frozenExpertEligible(status,'expert','[]',false),false);
assert.equal(frozenExpertEligible(status,'expert',ctx,true),false,'requires STOP');
function walk(node,arr=[]){
 if(Array.isArray(node)){node.forEach(x=>walk(x,arr));return arr}
 if(!node||typeof node!=='object')return arr;
 arr.push(node);walk(node.props?.children,arr);return arr;
}
let updated=0,actions=[];
const options={
 status,source:{status:'not_started'},currentMode:'expert',brainContext:ctx,
 armed:false,disabled:false,onRefresh:()=>{updated++},
 onAction:async action=>{actions.push(action)}
};
const buttons=node=>walk(node).filter(x=>x.type==='button');
const open=FrozenCoordinatorOperations(options);
const btn=buttons(open);
const freeze=btn.find(x=>String(x.props.children).includes('تجميد'));
const stop=btn.find(x=>String(x.props.children).includes('إيقاف'));
const refresh=btn.find(x=>x.props.children==='تحديث التجربة من Core');
assert(freeze&&!freeze.props.disabled,'freeze enabled on eligible Demo STOPPED');
assert(stop&&stop.props.disabled,'stop cannot end nonexistent trial');
assert(refresh&&!refresh.props.disabled);
freeze.props.onClick();refresh.props.onClick();
assert.deepEqual(actions,['freeze']);assert.equal(updated,1);
const active=FrozenCoordinatorOperations({...options,source:{
 status:'active',bars_compared:31,
 paired_quote_paper:{status:'insufficient_clean_feasible_pairs',completed_pairs:31,
 clean_feasible_complete_pairs:0,quote_gap_affected_pairs:31,
 unknown_quote_gap_evidence_pairs:0,
 clean_feasible_frozen_mean_net_per_lot:null,clean_feasible_adaptive_mean_net_per_lot:null}
}});
const activeBtns=buttons(active);
assert(activeBtns.find(x=>String(x.props.children).includes('تجميد')).props.disabled);
const activeStop=activeBtns.find(x=>String(x.props.children).includes('إيقاف'));
assert(!activeStop.props.disabled);activeStop.props.onClick();
assert.deepEqual(actions,['freeze','stop']);
const stale=FrozenCoordinatorOperations({...options,status:{...status,gateway_fresh:false},
 source:{status:'active',paired_quote_paper:{completed_pairs:100,
 clean_feasible_frozen_mean_net_per_lot:999999.}}});
assert(buttons(stale).every(x=>x.props.disabled===true ||
 x.props.children==='تحديث التجربة من Core'),'no frozen policy action on stale account');
assert(!JSON.stringify(stale).includes('999999'),'stale report numbers must be hidden');
const terminal=read('terminal.tsx'),bridge=read('bridge.ts');
const tauri=fs.readFileSync(path.resolve(dir,'../../tauri/src/bridge.rs'),'utf8');
const core=fs.readFileSync(path.resolve(dir,'../../../crates/runtime/src/execution.rs'),'utf8');
const paper=fs.readFileSync(path.resolve(dir,'../../../crates/runtime/src/coordinator_paper.rs'),'utf8');
for(const needle of [
 "scopedGeneration.current.beginRead('frozenCoordinator')",
 "const mutation=beginWrite('frozen-coordinator')",
 "if(!writeFresh(mutation))return;",
 "<FrozenCoordinatorOperations status={!preview&&feed.broker?status:null}"
])assert(terminal.includes(needle),needle);
assert(bridge.includes("invoke('frozen_coordinator_action'"));
assert(tauri.includes('"/v1/execution/frozen-coordinator"'));
assert(tauri.includes('matches!(action.as_str(),"freeze"|"stop")'));
assert(core.includes('frozen_coordinator::freeze'));
assert(core.includes('!status.requested_run && !self.pending(db)?'));
assert(paper.includes('clean_feasible_complete_pairs'));
assert(paper.includes('unknown_quote_gap_evidence_pairs'));
assert(paper.includes('if clean_feasible<MIN_COMPLETE'));
console.log('PASS: P1-I Expert frozen study Demo-only broker guards, functional explicit actions, Core readback, clean-feasible evidence and stale account concealment');
