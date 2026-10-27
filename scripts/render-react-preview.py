"""Offline Chromium DOM screenshot of the genuine React UI-15 JSX structure.
Because the build sandbox is offline, temporary React 16 UMD + hook stubs and
placeholder Lucide glyphs substitute for React 18/npm. This is a visual
smoke test, NOT an installer screenshot or proof that Tauri compiled.
"""
import pathlib,re,json,shutil,subprocess
ROOT=pathlib.Path(__file__).resolve().parents[1]
UI=ROOT/'apps'/'terminal'
OUT=ROOT/'visual-proof'
OUT.mkdir(exist_ok=True)
shutil.copy2(UI/'src'/'reference.css',OUT/'reference.css')
shutil.copy2(UI/'src'/'ui15.css',OUT/'ui15.css')
(OUT/'assets').mkdir(exist_ok=True)
shutil.copy2(UI/'public'/'assets'/'assistant-portrait.png',OUT/'assets'/'assistant-portrait.png')
VENDOR=pathlib.Path('/opt/pyvenv/lib/python3.13/site-packages/nbclassic/static/components/react')
shutil.copy2(VENDOR/'react.production.min.js',OUT/'react-legacy-umd.js')
shutil.copy2(VENDOR/'react-dom.production.min.js',OUT/'react-dom-legacy-umd.js')
# Use the exact TypeScript compiler bundled in the sandbox, without fetching modules.
js_compiler=r'''const ts=require('/opt/nvm/versions/node/v22.16.0/lib/node_modules/typescript');
const fs=require('fs');const base=process.argv[2],output=process.argv[3];
const parts=['format.ts','mock.ts','chart.tsx','terminal.tsx'];
let built='';
for(const f of parts){let s=fs.readFileSync(base+'/'+f,'utf8');
 s=s.replace(/^import .*?;\s*$/gm,'');s=s.replace(/^export default /gm,'');s=s.replace(/^export (const|function|type|interface) /gm,'$1 ');
 s=s.replace(/^export \{.*?\};\s*$/gm,'');
 const t=ts.transpileModule(s,{fileName:f,compilerOptions:{jsx:ts.JsxEmit.React,target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.None}});
 if(t.diagnostics?.length)throw Error(f+JSON.stringify(t.diagnostics));
 built+=t.outputText+'\n';
}
fs.writeFileSync(output,built+'\nReactDOM.render(React.createElement(Terminal), document.getElementById("root"));\nsetTimeout(()=>{ for(const cb of window.__effects||[]) try{cb()}catch(e){console.warn(e)} },70);');'''
comp=OUT/'compile.cjs';comp.write_text(js_compiler)
subprocess.run(['node',str(comp),str(UI/'src'),str(OUT/'app.js')],check=True)
bootstrap='''window.__effects=[];
const {createElement}=React;
let ix=0;
function useState(initial){const index=++ix;return [index===4?true:(typeof initial==='function'?initial():initial),()=>{}]}
function useEffect(effect){window.__effects.push(effect)}
function useMemo(fn){return fn()}
function useRef(){return {current:null}}
const hasTauri=()=>false;
const coreStatus=async()=>{throw Error('Offline screenshot')};const readPanel=async()=>null;
const subscribe=async()=>null;const control=async()=>{};const saveRisk=async()=>{};
const saveSettings=async()=>{};const handoff=async()=>{};
function ic(name){return function(props){return React.createElement('svg',Object.assign({width:21,height:21,viewBox:'0 0 24 24',stroke:'currentColor',strokeWidth:1.7,fill:'none',strokeLinecap:'round'},props),
 React.createElement('rect',{x:3,y:3,width:18,height:18,rx:4}),React.createElement('path',{d:'M7 15l3-4 3 2 4-5'}))}}
const names=['Activity','BarChart3','BookOpen','BrainCircuit','CalendarClock','ChartCandlestick','CheckCircle2','CircleHelp','Database','FileText','History','Home','Layers3','MessageCircle','PauseCircle','Play','Radio','RefreshCw','Settings','ShieldCheck','Siren','Target','Workflow','Zap','Wifi','WifiOff','SlidersHorizontal','Server','RotateCcw','LineChart','Clock4','BriefcaseBusiness','NotebookTabs','Eye','Sun','Cloud','Moon'];
for(const name of names)window[name]=ic(name);
'''
(OUT/'bootstrap.js').write_text(bootstrap)
(OUT/'index.html').write_text('<!DOCTYPE html><html lang="ar" dir="ltr"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="stylesheet" href="reference.css"><link rel="stylesheet" href="ui15.css"></head><body data-theme="dark"><div id="root"></div><script src="react-legacy-umd.js"></script><script src="react-dom-legacy-umd.js"></script><script src="bootstrap.js"></script><script src="app.js"></script></body></html>')
print(OUT/'index.html')
