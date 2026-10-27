const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),ts=require('typescript');
const file=path.resolve(__dirname,'../src/strategy-studio.tsx');
const compiled=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX}});
const jsx=(type,props)=>({type,props:props||{}}),exportsObj={};
vm.runInNewContext(compiled.outputText,{exports:exportsObj,require:id=>id==='react'?{useState:v=>[v,()=>{}]}:id==='react/jsx-runtime'?{jsx,jsxs:jsx}:(()=>{throw Error(id)})(),Date,Number,String,JSON});
function nodes(v,out=[]){if(Array.isArray(v))v.forEach(x=>nodes(x,out));else if(v&&typeof v==='object'){out.push(v);nodes(v.props?.children,out)}return out}
const labels=v=>{if(typeof v==='string')return v;if(Array.isArray(v))return v.map(labels).join(' ');return v&&typeof v==='object'?labels(v.props?.children):''};
const status={gateway_fresh:true,requested_run:false,telemetry:{connected:true,demo:true,account_trade_mode:0,server:'Demo',login:7,symbol:'XAUUSD',currency:'USD'}};
const context=JSON.stringify(['Demo',7,'XAUUSD','USD']);
const metric={trades:20,net_r:3,max_drawdown_r:2,costs_per_lot:140};
const source={context,connected:true,trading_mode:'expert',frames:[{timeframe:'M1',bars:600}],auto:false,reports:[{id:'candidate',created_ms:1000,report:{eligible_for_review:true,trials:50,training:metric,validation:metric,test:metric,selected:{family:'breakout'},evolution:{generations:3,rejection_reasons:[],selected_trial:0,best_training_trial:0,worst_training_trial:1,trials:[{id:0,generation:0,genome:{family:'breakout',lookback:12,stop_atr:1,reward:2},training_score:3,validation_score:2}]}}}]};
const calls=[],props={source,status,disabled:false,onAction:async(...x)=>calls.push(x),onRefresh:()=>{}};
let view=exportsObj.StrategyStudioOperations(props),buttons=nodes(view).filter(v=>v.type==='button');
for(const b of buttons.filter(v=>!v.props.disabled))b.props.onClick();
assert.deepEqual(calls.map(x=>x[0]),['run','auto_on','cancel','adopt_demo']);
assert.deepEqual(calls.find(x=>x[0]==='adopt_demo'),['adopt_demo',context,'candidate']);
assert(labels(view).includes('ليست أرباح MT5'));assert(labels(view).includes('المحاولات والتحسينات'));
const adopt=tree=>nodes(tree).find(v=>v.type==='button'&&String(v.props.children).includes('اعتماد المرشح'));
for(const change of [{requested_run:true},{telemetry:{...status.telemetry,demo:false,account_trade_mode:2}},{telemetry:{...status.telemetry,login:8}},{gateway_fresh:false}]){
 view=exportsObj.StrategyStudioOperations({...props,status:{...status,...change}});
 if(adopt(view))assert(adopt(view).props.disabled,'stale/running/REAL account cannot approve');
}
view=exportsObj.StrategyStudioOperations({...props,source:{...source,trading_mode:'sniper'}});assert(adopt(view).props.disabled);
view=exportsObj.StrategyStudioOperations({...props,source:{...source,reports:[{...source.reports[0],report:{...source.reports[0].report,eligible_for_review:false,evolution:{rejection_reasons:['final_test_insufficient_trades','validation_nonpositive_after_costs']}}}]}});
assert(adopt(view).props.disabled);assert(labels(view).includes('العينة غير كافية'));assert(labels(view).includes('غير موجبة بعد التكاليف'));
view=exportsObj.StrategyStudioOperations({...props,source:null});assert(!labels(view).includes('مرشّح للمراجعة'));assert(!adopt(view));
view=exportsObj.StrategyStudioOperations({...props,source:{...source,auto:true}});
nodes(view).find(v=>v.type==='button'&&String(v.props.children).includes('إيقاف البحث التلقائي')).props.onClick();assert.equal(calls.at(-1)[0],'auto_off');
console.log('PASS: Strategy Studio real JSX run/auto/cancel/adopt dispatch, account and mode fences, research labels and rejection reasons');
