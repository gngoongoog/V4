const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const ts=require('typescript');
const root=path.resolve(__dirname,'..');
const source=fs.readFileSync(path.join(root,'src/operations.tsx'),'utf8');
const output=ts.transpileModule(source,{fileName:'operations.tsx',compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX},reportDiagnostics:true});
const errors=(output.diagnostics||[]).filter(x=>x.category===ts.DiagnosticCategory.Error);
assert.equal(errors.length,0,errors.map(e=>ts.flattenDiagnosticMessageText(e.messageText,'\n')).join('\n'));
const jsx=(type,props)=>({type,props:props||{}});
const hooks={useState:initial=>[initial,()=>{}],useEffect:()=>{},useRef:initial=>({current:initial})};
const exportsObj={};
const catalog={};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,'src/brain-catalog.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:catalog});
vm.runInNewContext(output.outputText,{exports:exportsObj,require:id=>{
 if(id==='react')return hooks;
 if(id==='react/jsx-runtime')return {jsx,jsxs:jsx};
 if(id==='lucide-react')return Object.fromEntries(['Archive','BarChart3','BrainCircuit','CalendarClock','Download','Layers3','Play','Square','ShieldAlert','Target','TrendingUp','Zap'].map(n=>[n,n]));
 if(id==='./brain-catalog')return catalog;
 throw Error('untrusted module '+id);
},Date,Number,String,JSON,console});
function nodes(node,out=[]){if(!node||typeof node!=='object')return out;
 if(Array.isArray(node)){node.forEach(n=>nodes(n,out));return out;}
 out.push(node);nodes(node.props?.children,out);return out;
}
(async()=>{
 let started,stopped=0;
 const backtest=exportsObj.BacktestOperations({onStart:async x=>{started=x},onCancel:async()=>{stopped++},disabled:false,active:false});
 const form=nodes(backtest).find(x=>x.type==='form');
 assert(form&&typeof form.props.onSubmit==='function','real backtest form present');
 await form.props.onSubmit({preventDefault(){}});
 await Promise.resolve();
 assert(started&&started.from_date<started.to_date&&started.initial_equity===1000&&started.spread_floor===0.2);
 const cancel=nodes(backtest).find(x=>x.type==='button'&&x.props.type==='button');
 await cancel.props.onClick();assert.equal(stopped,1);
 const called=[];
 const backup=exportsObj.BackupOperations({create:async()=>called.push('create'),exportBrain:async()=>called.push('export'),importBrain:async()=>called.push('import'),source:{backups:[]},onRestore:()=>called.push('restore'),disabled:false,armed:false});
 for(const btn of nodes(backup).filter(x=>x.type==='button'&&!x.props.disabled))await btn.props.onClick();
 assert.deepEqual(called,['create','export','import']);
 assert(nodes(backup).some(x=>x.type==='button'&&String(x.props.children).includes('استعادة النسخة')&&x.props.disabled));
 let saved=null;
 for(const source of [null,{}, {config:{trading:{}}}]){
  const empty=exportsObj.TradingSettingsOperations({source,disabled:false,armed:false,apply:async()=>{throw Error('Unavailable source must not save')}});
  const label=nodes(empty).find(x=>x.type==='small');
  assert(String(label.props.children).includes('غير متاح'),'Missing Core settings must not be labeled as loaded');
  const action=nodes(empty).find(x=>x.type==='button'&&String(x.props.children).includes('حفظ الوضع'));
  assert(action.props.disabled,'Cannot save absent Core configuration');
 }
 const settings=exportsObj.TradingSettingsOperations({source:{config:{trading:{mode:'expert',priority_enabled:false,sniper_active:true,fallback_mode:'expert',sniper_style:'auto',sniper_brain:'classic',sniper_target_mode:'fixed',sniper_target_points:50,sniper_stop_points:80,pip_points:10,sniper_pip_price:0.1,sniper_cooldown_seconds:0,sniper_max_hold_seconds:120,sniper_min_target_pips:2,sniper_max_target_pips:10,max_cost_fraction:0.45,demo_flexible:true,adaptive_exits:true},risk:{slippage_ticks:2}},commission_editor:{amount:7,basis:'round_trip',account:{login:123,server:'Demo',symbol:'XAUUSD',currency:'USD'}}},disabled:false,armed:false,apply:async v=>{saved=v}});
 assert(String(nodes(settings).find(x=>x.type==='small').props.children).includes('إعدادات Core'));
 const save=nodes(settings).filter(x=>x.type==='button').find(x=>String(x.props.children).includes('حفظ الوضع'));
 assert(save,'trading editor has Core save action');await save.props.onClick();
 assert(saved&&saved.commission_per_lot===7&&saved.commission_basis==='round_trip'&&saved.trading.mode==='expert');
 assert.equal(saved.trading.sniper_target_points,50);
 assert.equal(saved.trading.sniper_stop_points,80);
 assert.equal(saved.trading.max_cost_fraction,0.45);
 assert.equal(saved.account.login,123,'explicit MT5 account must be sent for commission binding');
 assert(nodes(settings).some(x=>x.props?.['aria-label']==='sniper fixed target pips'));
 assert(nodes(settings).some(x=>x.props?.['aria-label']==='sniper cooldown seconds'));
 let brainState=null;
 const brains=exportsObj.BrainControlsOperations({source:{controls_available:true,brains:[{name:'system',config:{id:'system'}},{name:'Trend',config:{id:'trend'},control:{advisor:true,paper:false}}]},disabled:false,armed:false,onSave:async(...v)=>{brainState=v}});
 const brainButtons=nodes(brains).filter(x=>x.type==='button');
 const advisor=brainButtons.find(x=>String(x.props.children).includes('المشاركة:'));
 const paper=brainButtons.find(x=>String(x.props.children).includes('Paper:'));
 assert(advisor&&paper,'sub-brain advisor and Paper controls remain available');
 await advisor.props.onClick();assert.deepEqual(brainState,['trend',false,false]);
 await paper.props.onClick();assert.deepEqual(brainState,['trend',true,true]);
 const allChanges=[];
 const paperSource=(trend,gann)=>({controls_available:true,account_matches:true,brains:[
  {config:{id:'system'}},{config:{id:'trend'},control:{paper:trend}},{config:{id:'gann'},control:{paper:gann}}
 ]});
 for(const [source,expected] of [[paperSource(true,true),false],[paperSource(false,false),true],[paperSource(true,false),true]]){
  const bulk=exportsObj.PaperAllControl({source,disabled:false,armed:false,onSet:async paper=>allChanges.push(paper)});
  const button=nodes(bulk).find(x=>x.type==='button');
  assert(!button.props.disabled);await button.props.onClick();assert.equal(allChanges.pop(),expected);
 }
 for(const source of [null,{...paperSource(true,true),account_matches:false},paperSource(true,undefined)]){
  const bulk=exportsObj.PaperAllControl({source,disabled:false,armed:false,onSet:async()=>{throw Error('unverified Paper control')}});
  assert(nodes(bulk).find(x=>x.type==='button').props.disabled,'unverified Paper state cannot dispatch a bulk change');
 }
 assert(nodes(exportsObj.PaperAllControl({source:paperSource(true,true),disabled:false,armed:true,onSet:async()=>{}})).find(x=>x.type==='button').props.disabled,'Paper controls wait for STOP');
 const offline=exportsObj.BrainControlsOperations({source:null,disabled:false,armed:false,onSave:async()=>{throw Error('offline control must never execute')}});
 const offlineCards=nodes(offline).filter(x=>x.type==='article'&&String(x.props.className).includes('concept-brain-card'));
 assert.equal(offlineCards.length,16,'offline directory shows the actual secondary brain identities');
 for(const card of offlineCards){
  const controls=nodes(card).filter(x=>x.type==='button'&&String(x.props.children).includes(':'));
  assert.equal(controls.length,2);assert(controls.every(b=>b.props.disabled),'offline controls must be disabled');
  assert(nodes(card).some(x=>x.props?.className==='concept-control-state '&&x.props.children==='غير موثق'));
 }
 let bound=null;
 const broker=exportsObj.ExecutionBindingOperations({source:{settings:{target:'demo',login:0}},telemetry:{demo:true,account_trade_mode:0,login:12345,server:'Demo-Server',currency:'USD',symbol:'XAUUSD'},ready:true,armed:false,disabled:false,onBind:async(...v)=>bound=v});
 await nodes(broker).find(x=>x.type==='form').props.onSubmit({preventDefault(){}});
 assert.deepEqual(bound,['demo',20,''],'Demo account binding must use observed broker mode');
 const priorState=hooks.useState;
 hooks.useState=(()=>{const data=['live','20','wrong-login',''];let i=0;return initial=>[data[i++]??initial,()=>{}]})();
 bound=null;
 const badLive=exportsObj.ExecutionBindingOperations({source:{settings:{}},telemetry:{demo:false,account_trade_mode:2,login:982134,server:'REAL',currency:'USD',symbol:'XAUUSD'},ready:true,armed:false,disabled:false,onBind:async(...v)=>bound=v});
 await nodes(badLive).find(x=>x.type==='form').props.onSubmit({preventDefault(){}});
 assert.equal(bound,null,'REAL binding must reject incorrect login');
 hooks.useState=(()=>{const data=['live','20','982134',''];let i=0;return initial=>[data[i++]??initial,()=>{}]})();
 const goodLive=exportsObj.ExecutionBindingOperations({source:{settings:{}},telemetry:{demo:false,account_trade_mode:2,login:982134,server:'REAL',currency:'USD',symbol:'XAUUSD'},ready:true,armed:false,disabled:false,onBind:async(...v)=>bound=v});
 await nodes(goodLive).find(x=>x.type==='form').props.onSubmit({preventDefault(){}});
 assert.deepEqual(bound,['live',20,'982134'],'REAL bind requires exact observed MT5 login');
 hooks.useState=priorState;
 const rs=fs.readFileSync(path.join(root,'../tauri/src/bridge.rs'),'utf8');
 const core=fs.readFileSync(path.join(root,'../../crates/runtime/src/lib.rs'),'utf8');
 for(const route of ['/v1/backup','/v1/backtest/start','/v1/execution/brain-control','/v1/execution/settings'])assert(rs.includes(route),'native allowlist missing '+route);
 assert(rs.includes('client.shutdown(&instance)')&&core.includes('/v1/shutdown'),'bound Core shutdown path missing');
 assert(core.includes('verify_safe_shutdown(&current,unresolved)?'));
 assert(rs.includes('state.stopping.store(true,Ordering::SeqCst)'));
 assert(!rs.includes('fn arbitrary_post'));
 console.log('PASS: actual JSX actions dispatch backtest/backup/modes/brains; system brain immutable; safe exit wired; explicit verified Demo/REAL broker account binding');
})().catch(e=>{console.error(e);process.exitCode=1});
