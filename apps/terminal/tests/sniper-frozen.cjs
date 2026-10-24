const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),ts=require('typescript');
const src=path.resolve(__dirname,'../src');
const read=p=>fs.readFileSync(path.join(src,p),'utf8');
const js=ts.transpileModule(read('sniper-frozen-operations.tsx'),{
 fileName:'sniper-frozen-operations.tsx',
 compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}
}).outputText;
const component={},jsx=(type,props)=>({type,props:props||{}});
vm.runInNewContext(js,{exports:component,require:id=>{
 if(id==='react/jsx-runtime')return {jsx,jsxs:jsx};
 throw Error('Unknown UI module '+id)
},JSON,Date,Number,String,Object});
const {eligibleFrozenSniper,SniperFrozenOperations}=component;
const t={connected:true,demo:true,account_trade_mode:0,login:17,
 server:'Demo',symbol:'XAUUSD',currency:'USD',source_quote_time_ms:101000};
const status={gateway_fresh:true,requested_run:false,telemetry:t};
const page={config:{trading:{mode:'sniper',sniper_brain:'learner'}}};
const context=JSON.stringify(['Demo',17,'XAUUSD','USD']);
const report={schema:'p1k3-sniper-frozen-tick-v1',status:'active',
 context,mode:'sniper',sniper_brain:'learner',
 no_broker_orders:true,independent_oos:false,paper_pnl:null,broker_pnl:null,
 accepted_prospective_pulses:60,comparable_future_tick_decisions:22,
 exact_decisions:18,side_disagreements:1,
 readiness_disagreements:2,target_or_stop_disagreements:1};
const clone=v=>JSON.parse(JSON.stringify(v));
assert.equal(eligibleFrozenSniper(status,page),true);
for(const [label,change] of [
 ['real',(s,p)=>{s.telemetry.demo=false;s.telemetry.account_trade_mode=2}],
 ['stale',(s,p)=>{s.gateway_fresh=false}],
 ['disconnected',(s,p)=>{s.telemetry.connected=false}],
 ['classic',(s,p)=>{p.config.trading.sniper_brain='classic'}],
 ['other mode',(s,p)=>{p.config.trading.mode='expert'}]
]){
 const s=clone(status),p=clone(page);change(s,p);
 assert.equal(eligibleFrozenSniper(s,p),false,label);
}
function walk(n,list=[]){if(Array.isArray(n)){n.forEach(x=>walk(x,list));return list}
 if(!n||typeof n!=='object')return list;
 list.push(n);walk(n.props?.children,list);return list;
}
const buttons=render=>walk(render).filter(n=>n.type==='button');
let called=[],refreshed=0;
const props={status,page,report,preview:false,disabled:false,
 onRefresh:()=>refreshed++,
 onAction:async action=>called.push(action)};
const active=SniperFrozenOperations(props);
const a=buttons(active);
assert.equal(a.length,3);
assert(a[1].props.disabled);
assert(!a[2].props.disabled);
a[2].props.onClick();a[0].props.onClick();
assert.deepEqual(called,['stop']);assert.equal(refreshed,1);
const none=SniperFrozenOperations({...props,report:{schema:'p1k3-sniper-frozen-tick-v1',
 status:'not_started'}});
const b=buttons(none);assert(!b[1].props.disabled);
assert(b[2].props.disabled);b[1].props.onClick();
assert.deepEqual(called,['stop','freeze']);
const invalid=SniperFrozenOperations({...props,report:{...report,context:'[]',
 accepted_prospective_pulses:999999}});
assert(!JSON.stringify(invalid).includes('999999'),'previous account score must disappear');
const otherAccount=SniperFrozenOperations({...props,status:{...status,telemetry:{...t,login:18}}});
assert(!JSON.stringify(otherAccount).includes('22'),'old account parity must be concealed');
assert(buttons(otherAccount).slice(1).every(x=>x.props.disabled||x.props.children.includes('تجميد')),
 'no stop action against old-account report');
const real=SniperFrozenOperations({...props,status:{...status,telemetry:{...t,demo:false,account_trade_mode:2}}});
assert(buttons(real).slice(1).every(x=>x.props.disabled));
const running=SniperFrozenOperations({...props,status:{...status,requested_run:true}});
assert(buttons(running).slice(1).every(x=>x.props.disabled),'STOP required');
const tauri=fs.readFileSync(path.resolve(src,'../../tauri/src/bridge.rs'),'utf8');
const rust=fs.readFileSync(path.resolve(src,'../../../crates/runtime/src/execution.rs'),'utf8');
const research=fs.readFileSync(path.resolve(src,'../../../crates/runtime/src/sniper_frozen_tick.rs'),'utf8');
const terminal=read('terminal.tsx');
assert(tauri.includes('fn frozen_sniper_action('));
assert(tauri.includes('"/v1/execution/frozen-sniper"'));
assert(terminal.includes('<SniperFrozenOperations status={!preview&&feed.broker?status:null}'));
assert(terminal.includes("scopedGeneration.current.beginRead('sniperFrozen')"));
assert(terminal.includes("const mutation=beginWrite('sniper-frozen')"));
assert(rust.includes('self.sniper_frozen_tick.freeze('));
assert(rust.includes('self.sniper_frozen_tick.observe('));
assert(research.includes('independent_oos":false'));
assert(research.includes('paper_pnl":Value::Null'));
assert(!research.includes('order_send('));
console.log('PASS: P1-K3 account-isolated, Demo Learner-only frozen Tick research, truthful no-PnL, functional stop/freeze and scoped UI readback');
