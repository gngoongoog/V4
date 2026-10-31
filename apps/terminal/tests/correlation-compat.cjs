// Guard the native Core correlation rules before users see a clickable research action.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require('typescript');
const src=fs.readFileSync(path.join(__dirname,'../src/correlation-workspace.tsx'),'utf8');
const transpiled=ts.transpileModule(src,{fileName:'correlation-workspace.tsx',
 compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX},
 reportDiagnostics:true});
assert(!transpiled.diagnostics?.some(d=>d.category===ts.DiagnosticCategory.Error),'correlation TSX must transpile');
const api={};
vm.runInNewContext(transpiled.outputText,{exports:api,require:name=>{
 if(name==='react')return {useState:value=>[value,()=>{}],useEffect:()=>{},useCallback:f=>f};
 if(name==='react/jsx-runtime')return {jsx:()=>null,jsxs:()=>null};
 if(name==='lucide-react')return {BarChart3:'BarChart3',BookOpen:'BookOpen',RefreshCw:'RefreshCw',ShieldAlert:'ShieldAlert'};
 if(name==='./bridge')return {correlationAction:()=>{throw Error('must not invoke Core in source rule test')}};
 if(name.endsWith('.css'))return {};
 throw Error('unexpected import '+name);
},Date,Number,Math,String,console});
const base={id:'saved-AAA',symbol:'XAUUSD',server:'Demo',login:42,currency:'USD',from:1000,to:10000};
const good={...base,id:'saved-BBB',symbol:'EURUSD',from:1500,to:9500};
const matches=(a,b)=>api.sourceCompatibility(a,b);
assert.equal(matches([],base),null,'first qualified saved source is selectable');
assert.equal(matches([base],good),null,'distinct symbol same-account overlapping history is selectable');
assert.match(matches([base],{...good,symbol:'XAUUSD'}),/رمزين مختلفين/);
assert.match(matches([base],{...good,login:43}),/نفس حساب/);
assert.match(matches([base],{...good,server:'Different'}),/نفس حساب/);
assert.match(matches([base],{...good,currency:'EUR'}),/نفس حساب/);
assert.match(matches([base],{...good,from:10000,to:11000}),/لا تتداخل/);
assert.match(matches([],{...base,login:undefined}),/ناقص/);
assert.match(matches([],{...base,from:1111,to:1111}),/ناقص/);
assert.equal(matches([base,good],{...good,id:'saved-CCC',symbol:'GBPUSD',from:2000,to:9000}),null);
assert.match(src,/XAUUSD وحده لا تكفي/,'UI must warn that gold-only history cannot make multi-symbol correlation');
assert.match(src,/pair\.reason/,'insufficient-overlap details from the Core report should be visible');
console.log('PASS: correlation selection mirrors Core distinct-symbol same-account same-currency and overlapping-date requirements');
