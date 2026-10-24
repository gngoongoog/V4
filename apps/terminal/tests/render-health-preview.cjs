/** UI-16 visual harness: compiles SystemHealth's actual TSX and renders its JSX
 * component tree without external React packages. Surrounding MAIN region is
 * the verified UI-15 reference screenshot, NOT a live Tauri render.
 */
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const ts=require('typescript');
const root=path.resolve(__dirname,'..');
const original=fs.readFileSync(path.join(root,'src/system-health.tsx'),'utf8');
const js=ts.transpileModule(original,{compilerOptions:{module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX,target:ts.ScriptTarget.ES2020}}).outputText;
const icons=['Activity','ArrowUpRight','Clock3','Cpu','DatabaseZap','ShieldCheck','Wifi','Workflow'];
const moduleExports={};
vm.runInNewContext(js,{exports:moduleExports,require:id=>id==='lucide-react'?Object.fromEntries(icons.map(x=>[x,'svg:'+x])):{jsx:(type,props)=>({type,props}),jsxs:(type,props)=>({type,props})}});
const health=moduleExports.buildHealth(null,false,true,false,null,Date.now());
const vtree=moduleExports.default({health,onOpen:()=>{}});
function escape(s){return String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
const paths={Activity:'<path d="M22 12h-4l-3 9L9 3l-3 9H2"/>',ArrowUpRight:'<path d="M7 17 17 7M7 7h10v10"/>',Clock3:'<circle cx="12" cy="12" r="10"/><path d="M12 6v6h4"/>',Cpu:'<rect x="4" y="4" width="16" height="16" rx="2"/><rect x="8" y="8" width="8" height="8"/><path d="M9 1v3m6-3v3M9 20v3m6-3v3M1 9h3m-3 6h3m16-6h3m-3 6h3"/>',Wifi:'<path d="M2 8.82a15 15 0 0 1 20 0M5 12.86a10 10 0 0 1 14 0M8.5 16.43a5 5 0 0 1 7 0M12 20h.01"/>',DatabaseZap:'<ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5v14c0 1.8 3.5 3 9 3M21 5v7M3 12c0 4 14 4 18 0M13 17l-2 4h4l-2 3"/>',ShieldCheck:'<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Z"/><path d="m9 12 2 2 4-4"/>',Workflow:'<rect x="2" y="2" width="8" height="8" rx="2"/><path d="M6 10v5a4 4 0 0 0 4 4h4"/><rect x="14" y="14" width="8" height="8" rx="2"/>'};
function render(t){if(t===null||t===undefined||typeof t==='boolean')return '';if(Array.isArray(t))return t.map(render).join('');if(typeof t!=='object')return escape(t);
 if(typeof t.type==='string'&&t.type.startsWith('svg:'))return `<svg width="${Number(t.props?.size)||16}" height="${Number(t.props?.size)||16}" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" viewBox="0 0 24 24" aria-hidden="true">${paths[t.type.slice(4)]||''}</svg>`;
 const attrs=Object.entries(t.props||{}).filter(([k])=>!['children','key','ref','onClick'].includes(k)).map(([k,v])=>` ${k==='className'?'class':k==='htmlFor'?'for':k}="${escape(v)}"`).join('');return `<${t.type}${attrs}>${render(t.props?.children)}</${t.type}>`;
}
const project=path.resolve(root,'../..');
const image=fs.readFileSync(path.join(project,'visual-proof/UI15_React_dashboard_1648x928.png')).toString('base64');
const screen='data:image/png;base64,'+image;
const style=fs.readFileSync(path.join(root,'src/reference.css'),'utf8')+'\n'+fs.readFileSync(path.join(root,'src/ui15.css'),'utf8');
const nav=['نظرة عامة','مركز التداول','وضع تلقائي','العقول','إدارة العقول','القناص','إدارة المخاطر','المراكز النشطة','التحليلات','السجلات','التقارير'];
const navMarkup=nav.map((n,i)=>`<button type="button" class="nav-item ${i===0?'selected':''}"><span class="nav-glyph">◈</span><span>${n}</span></button>`).join('');
const html=`<!doctype html><html><head><meta charset="utf-8"><style>${style}\nbody{margin:0}.terminal-v15 .content{overflow:hidden}.reference-screen{background:url('${screen}') -212px top / 1648px 928px no-repeat;height:100%;width:100%} .visual-note{position:absolute;bottom:38px;right:18px;border-radius:6px;background:#263b44e8;border:1px solid #427688;padding:8px 12px;color:#e4f8ff;font-size:12px;z-index:5} .terminal-v15 .nav-glyph{font-size:14px;color:#d6ebf4}</style></head><body data-theme="dark"><div class="shell terminal-v15"><aside class="sidebar"><div class="brand"><div class="brand-logo">BRAIN<span>TRADE</span> <b>V4</b></div><div class="brand-desc">Institutional Trading System</div></div><nav id="sidebarNav">${navMarkup}<button class="nav-item all-pages"><span>◈</span><span>كل الصفحات الـ22</span></button></nav>${render(vtree)}<div class="sidebar-footer"><strong>BRAIN<span>TRADE</span> V4</strong><small>Tauri 2 + React + Rust</small><button class="compact-toggle">طي القائمة</button></div></aside><div class="main"><div class="reference-screen"></div></div></div><div class="visual-note">UI-16 System Health component render • Main area from UI-15 reference screenshot • PREVIEW</div></body></html>`;
const out=path.join(project,'visual-proof','UI16_SystemHealth_component_preview.html');fs.writeFileSync(out,html,'utf8');
console.log(out);
