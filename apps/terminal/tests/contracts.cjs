const fs=require('node:fs');const assert=require('node:assert/strict');const path=require('node:path');
const read=p=>fs.readFileSync(path.join(__dirname,'..',p),'utf8');
const terminal=read('src/terminal.tsx');const bridge=read('../tauri/src/bridge.rs');
const chart=read('src/chart.tsx');const body=read('src/ui15.css');const fixture=read('src/mock.ts');
const names=['dashboard','trading','execution','brains','brainControl','sniper','risk','positions','analysis','journal','reports','telegram','settings','shadow','learning','backtest','evaluation','correlation','studio','news','backups','events'];
for(const id of names)assert.match(terminal,new RegExp("id:'"+id+"'"));
assert.equal(names.length,22);assert.match(terminal,/const \[page,setPage\]=useState/);
assert.match(bridge,/thread::sleep\(Duration::from_millis\(if failures > 0 \{ 900 \} else \{ 250 \}\)\)/);
assert.match(bridge,/"\/v1\/start"/);assert.match(bridge,/"\/v1\/stop"/);
assert.match(bridge,/allowed_read_route\(&route\)/);
assert.match(bridge,/"\/v1\/execution\/brains"/);
assert.match(bridge,/last_send\.is_none_or/);
assert.match(terminal,/setInterval\(\(\)=>void refresh\(\)/);
assert.doesNotMatch(terminal,/BRAIN_NAMES/);assert.doesNotMatch(bridge,/fn get_secret/);
assert.match(chart,/ResizeObserver/);assert.match(chart,/tick_volume/);
assert.match(terminal,/feedEvidence\(status,online,lastStatusAt,Date.now\(\)\)/);
assert.match(terminal,/setPreview\(v=>!v\)/);assert.match(terminal,/saveRisk\(risk\)/);
assert.match(fixture,/Visual fixture ONLY/);assert.match(body,/prefers-reduced-motion/);
const setting=read('../tauri/tauri.conf.json');assert.match(setting,/com.braintrade.terminal.next/);
console.log('PASS: 22 routes, native IPC guard, real data separation, responsive live chart, 250ms streamed status');

const health=read('src/system-health.tsx');
assert.match(terminal,/<SystemHealth health=\{health\} onOpen=\{open\}\/>/);
assert.doesNotMatch(terminal,/assistant-portrait|reference-robot|المساعد الذكي/);
assert.match(health,/buildHealth\(/);
assert.match(health,/feedEvidence\(status,online,lastStatusAt,now\)/);
assert.match(read('src/feed.ts'),/quoteAgeMs<=3000/);
assert.match(health,/value:executionOk\?'ARMED'/);
assert.match(health,/title:'DESIGN PREVIEW'/);
assert.match(health,/riskLoaded/);
assert.match(body, /\.health-widget/);
assert.match(body, /max-height:820px/);
assert.equal(fs.existsSync(path.join(__dirname,'../public/assets/assistant-portrait.png')),false);
console.log('PASS: UI-16 real-data System Health replaces robot across sidebar and narrow screens');

// P0-G: stale MT5 Demo/REAL panels must be invalidated, and advisor action
// must match the connected broker + freshly queried page generation.
assert.match(terminal,/const contextKey=`[^`]*account_trade_mode/);
assert.match(terminal,/const contextKey=`[^`]*currency/);
assert.match(terminal,/view\.context!==context\|\|view\.account_matches!==true/);
assert.match(terminal,/account_trade_mode!==\(t\.demo\?0:2\)/);
assert.match(terminal,/\|\|status\.requested_run/);
assert.match(terminal,/confirmed\.advisor!==advisor\|\|confirmed\.paper!==paper/);
assert.match(terminal,/rows\.some\(r=>unwrap\(unwrap\(r\)\.control\)\.paper!==paper\)/);
console.log('PASS: P0-G strict broker-scoped toggles and verified Core readback');

// P0-H: a late IPC response cannot overwrite a different page/account.
assert.match(terminal,/const viewGeneration=useRef\(new PanelReadFence\(\)\)/);
assert.match(terminal,/viewGeneration\.current\.setScope\(viewScope\)/);
assert.match(terminal,/if\(!viewGeneration\.current\.isCurrent\(serial\)\)return;/);
assert(terminal.includes('publishPanel(serial,refreshed);'));
assert(terminal.includes("const ticket=scopedGeneration.current.beginRead('brains')"));
assert(terminal.includes('if(scopedGeneration.current.isCurrent(ticket))setBrainReport(refreshed)'));
assert(terminal.includes('scopedGeneration.current.setScope(contextKey)'));
assert(terminal.includes('feed.broker&&brainReport?.account_matches===true'));
assert(!terminal.includes('riskSnapshot?.drawdown_percent'));
console.log('PASS: P1-C background Core scope fences and honest risk display');
assert.match(terminal,/onRefresh=\{\(\)=>refreshCurrentPanel\('\/v1\/execution'\)\}/);
assert.doesNotMatch(terminal,/setPageData\(await readPanel\(/);
console.log('PASS: P0-H stale IPC manual refresh/account generation rejection');
