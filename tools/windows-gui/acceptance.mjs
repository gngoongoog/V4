// Exercise the unmodified shipped EXE, not a browser recreation or IPC mock.
// Only run on an isolated hosted Windows runner. No MT5 connection or START.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import net from 'node:net';
import crypto from 'node:crypto';
import {spawn,spawnSync} from 'node:child_process';
import {chromium} from 'playwright-core';

const root=process.cwd(),exe=path.resolve(process.argv[2]||''),out=path.resolve(process.argv[3]||'test-output/v25/native-webview');
assert.equal(process.platform,'win32','Actual Windows is required');
assert.equal(process.env.GITHUB_ACTIONS,'true','Only an isolated GitHub hosted runner');
assert.equal(process.env.RUNNER_ENVIRONMENT,'github-hosted','Never change a user machine policy');
assert.equal(path.basename(exe),'BrainTradeNext.exe');
fs.mkdirSync(out,{recursive:true});
const expected=JSON.parse(fs.readFileSync(path.join(path.dirname(exe),'release-evidence/build.json'),'utf8'));
assert.equal(expected.source_commit,process.env.GITHUB_SHA,'Portable must come from this exact CI checkout');
assert.equal(crypto.createHash('sha256').update(fs.readFileSync(exe)).digest('hex'),expected.exe_sha256);
const home=fs.mkdtempSync(path.join(os.tmpdir(),'braintrade-rc2-gui-'));
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function portOpen(port){return new Promise(resolve=>{const s=net.connect({port,host:'127.0.0.1'});s.once('connect',()=>{s.destroy();resolve(true)});s.once('error',()=>resolve(false));s.setTimeout(1000,()=>{s.destroy();resolve(false)})})}
assert.equal(await portOpen(18741),false,'Refuse to touch an existing Core');
const port=await new Promise(resolve=>{const s=net.createServer();s.listen(0,'127.0.0.1',()=>{const n=s.address().port;s.close(()=>resolve(n))})});
const flags=`--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1 --disable-gpu`;
const policies=[];
function registry(kind,value){
 const key=`HKLM\\Software\\Policies\\Microsoft\\Edge\\WebView2\\${kind}`;
 const old=spawnSync('reg.exe',['query',key,'/v','BrainTradeNext.exe'],{encoding:'utf8'});
 assert.notEqual(old.status,0,'Refuse to replace existing per-application WebView2 policy');
 const created=spawnSync('reg.exe',['add',key,'/v','BrainTradeNext.exe','/t','REG_SZ','/d',value,'/f'],{encoding:'utf8'});
 assert.equal(created.status,0,created.stderr||created.stdout);
 policies.push(key);
}
const report={schema:1,source_commit:expected.source_commit,build_id:expected.build_id,
 exe_sha256:expected.exe_sha256,started_utc:new Date().toISOString(),
 runtime:'actual packaged Tauri + WebView2 + Rust Core',fixture_data:false,
 gateway_disabled:true,broker_demo:'NOT TESTED',native_ea:'NOT TESTED',
 all_controls_certified:false,ready_for_real:false,cases:[],page_errors:[]};
let app,browser,page,fixture,failed=false;
const log=fs.openSync(path.join(out,'native-process.log'),'a');
async function until(fn,ms=30000){const deadline=Date.now()+ms;let last;while(Date.now()<deadline){try{const v=await fn();if(v)return v}catch(e){last=e}await delay(200)}throw new Error('Timeout'+(last?': '+last.message:''))}
async function check(name,fn){
 const start=Date.now();try{await fn();report.cases.push({name,status:'PASS',duration_ms:Date.now()-start});console.log('PASS '+name)}
 catch(e){report.cases.push({name,status:'FAIL',error:String(e),duration_ms:Date.now()-start});throw e}
}
async function launch(){
 app=spawn(exe,[],{cwd:path.dirname(exe),stdio:['ignore',log,log],env:{...process.env,
 BRAINTRADE_DATA_DIR:home,BRAINTRADE_GATEWAY_DISABLED:'1',BRAINTRADE_NEWS_OFFLINE:'1',
 WEBVIEW2_USER_DATA_FOLDER:path.join(home,'webview'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:flags}});
 await until(async()=>{assert.equal(app.exitCode,null,'EXE exited before WebView became available');return (await fetch(`http://127.0.0.1:${port}/json/version`)).ok},45000);
 browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
 page=await until(()=>browser.contexts()[0]?.pages().find(p=>p.url().includes('tauri.localhost')||p.url().startsWith('tauri:')));
 page.setDefaultTimeout(20000);
 page.on('pageerror',e=>report.page_errors.push(String(e)));
 await page.bringToFront();
 spawnSync('powershell.exe',['-NoProfile','-Command',`(New-Object -ComObject WScript.Shell).AppActivate(${app.pid})`],{encoding:'utf8'});
 await page.waitForFunction(()=>document.visibilityState==='visible'&&!!window.__TAURI_INTERNALS__);
 await page.locator('#sidebarNav').waitFor();
}
const invoke=(command,args={})=>page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args});
async function status(){const s=await invoke('get_status');assert.equal(s.requested_run,false);assert.equal(s.execution_enabled,false);assert.equal(s.build.build_id,expected.build_id);return s}
async function navigate(title){
 const all=page.locator('#sidebarNav .all-pages');
 if(await all.getAttribute('aria-expanded')!=='true')await all.click();
 // Exact title selector avoids overlapping Arabic labels.
 const exact=page.locator('#sidebarNav').getByTitle(title,{exact:true});
 await exact.click();
 await until(async()=>await exact.getAttribute('aria-current')==='page');
 await page.locator('main.content').waitFor();
 assert((await page.locator('main.content').innerText()).trim().length>15,'Empty page');
}
async function trading(){await navigate('مركز التداول');const p=page.getByRole('region',{name:'Coordinator and costs'});await p.waitFor();await until(async()=>await p.getByRole('button',{name:'حفظ الوضع والعمولة لدى Core',exact:true}).isEnabled());return p}
async function readSettings(){const r=await invoke('read_panel',{route:'/v1/trading/settings'});return r.data||r}
async function startFixture(){
 assert(!fixture,'Synthetic feed already running');
 const python=path.join(path.dirname(exe),'runtime','python','python.exe');
 fixture=spawn(python,[path.join(root,'tools/windows-gui/synthetic_feed.py'),home],{
  cwd:root,stdio:['ignore',log,log],env:{...process.env,BRAINTRADE_GATEWAY_DISABLED:'1'}});
 fixture.on('error',e=>console.error('Synthetic GUI fixture failed: '+e.message));
 report.fixture_data=true;
 report.fixture_description='Synthetic CI Demo snapshot; actual gateway disabled; no broker orders';
 await until(async()=>{
  assert(fixture.exitCode===null&&fixture.signalCode===null,'Synthetic GUI fixture exited');
  const s=await status();return s.gateway_fresh===true&&s.telemetry?.server==='Synthetic-CI-Demo';
 });
}
async function stopFixture(){
 if(!fixture)return;
 const child=fixture;fixture=undefined;
 if(child.exitCode===null&&child.signalCode===null){
  child.kill();await until(()=>child.exitCode!==null||child.signalCode!==null,10000);
 }
}
async function exitSafely(){await page.getByRole('button',{name:'خروج كامل آمن',exact:true}).click();await page.getByRole('dialog',{name:'خروج كامل آمن'}).getByRole('button',{name:'طلب الخروج الآمن'}).click();await until(()=>app.exitCode!==null);await until(async()=>!await portOpen(18741)&&!await portOpen(port));browser=undefined;page=undefined}

try{
 // Elevated Windows runners ignore environment-only overrides. Policies are
 // scoped to this executable, only in disposable CI, and removed in finally.
 registry('AdditionalBrowserArguments',flags);
 registry('UserDataFolder',path.join(home,'webview'));
 await check('packaged EXE starts actual Tauri/Core STOPPED',async()=>{await launch();await status();await page.locator('.bottom-status').getByText('Enterprise V25 RC2',{exact:false}).waitFor()});
 await check('offline START disabled and STOP dialog Escape',async()=>{
  await navigate('نظرة عامة');assert(await page.locator('button.action-btn.start').isDisabled());
  await page.locator('button.action-btn.stop').click();await page.getByRole('dialog',{name:'تأكيد وقف الدخول'}).waitFor();
  await page.keyboard.press('Escape');await page.getByRole('dialog').waitFor({state:'hidden'});await status();
 });
 await check('STOP click reaches Core without starting execution',async()=>{
  await page.locator('button.action-btn.stop').click();await page.getByRole('button',{name:'تأكيد وقف الدخول الجديد',exact:true}).click();
  await page.getByRole('dialog').waitFor({state:'hidden'});await status();
 });
 const source=fs.readFileSync(path.join(root,'apps/terminal/src/terminal.tsx'),'utf8');
 const pages=[...source.matchAll(/\{id:'([^']+)',title:'([^']+)',en:'([^']+)'/g)].map(m=>({id:m[1],title:m[2]}));
 assert.equal(pages.length,22,'Expected all 22 source-registered pages');
 for(const item of pages)await check('navigate '+item.id,async()=>{
  await navigate(item.title);await page.screenshot({path:path.join(out,item.id+'.png')});await status();
 });
 await check('G7 sealing is disabled without bound fresh Demo',async()=>{
  await navigate('إدارة العقول');assert(await page.getByRole('button',{name:'ختم نسخة Candidate البحثية',exact:true}).isDisabled());
 });
 await check('trading settings are disabled when broker is offline',async()=>{
  await navigate('مركز التداول');
  const panel=page.getByRole('region',{name:'Coordinator and costs'});await panel.waitFor();
  assert(await panel.getByRole('button',{name:'حفظ الوضع والعمولة لدى Core',exact:true}).isDisabled());
  await status();
 });
 await check('connect isolated synthetic Demo snapshot',startFixture);
 await check('commission draft survives disconnect and same-account reconnect',async()=>{
  let panel=await trading();await panel.getByLabel('commission per lot',{exact:true}).fill('9');
  await stopFixture();
  await until(async()=>!(await status()).gateway_fresh);
  panel=page.getByRole('region',{name:'Coordinator and costs'});
  await until(async()=>await panel.getByRole('button',{name:'حفظ الوضع والعمولة لدى Core',exact:true}).isDisabled());
  assert.equal(await panel.getByLabel('commission per lot',{exact:true}).inputValue(),'9');
  await startFixture();panel=await trading();
  assert.equal(await panel.getByLabel('commission per lot',{exact:true}).inputValue(),'9');
  await panel.getByRole('button',{name:'التراجع عن التعديلات',exact:true}).click();
 });
 for(const mode of ['scalping','swing','expert','sniper'])await check('save '+mode+' and commission through real UI',async()=>{
  const panel=await trading();await panel.getByRole('checkbox').nth(0).uncheck();
  await panel.locator('select').nth(0).selectOption(mode);
  await panel.getByLabel('commission per lot',{exact:true}).fill('7');
  await panel.getByLabel('commission basis',{exact:true}).selectOption('round_trip');
  await panel.getByRole('button',{name:'حفظ الوضع والعمولة لدى Core',exact:true}).click();
  await until(async()=>{const r=await readSettings();return r.config?.trading?.mode===mode&&r.commission_editor?.amount===7&&r.commission_editor?.basis==='round_trip'});
  await status();
 });
 await check('Classic and Learner persist independently',async()=>{
  for(const brain of ['classic','learner']){
   const panel=await trading();await panel.locator('select').nth(3).selectOption(brain);
   await panel.getByRole('button',{name:'حفظ الوضع والعمولة لدى Core',exact:true}).click();
   await until(async()=>(await readSettings()).config?.trading?.sniper_brain===brain);
  }
 });
 await check('commission unsaved edits can be discarded',async()=>{
  const panel=await trading();await panel.getByLabel('commission per lot',{exact:true}).fill('999');
  await panel.getByRole('button',{name:'التراجع عن التعديلات',exact:true}).click();
  assert.equal(await panel.getByLabel('commission per lot',{exact:true}).inputValue(),'7');
  assert.equal((await readSettings()).commission_editor.amount,7);
 });
 await check('preview is explicit and cannot start trading',async()=>{
  await navigate('نظرة عامة');await page.getByRole('button',{name:'معاينة التصميم',exact:true}).click();
  await page.locator('.preview-banner').waitFor();assert(await page.locator('button.action-btn.start').isDisabled());
  await navigate('إدارة المخاطر');const riskInputs=page.locator('.risk-editor input');
  assert(await riskInputs.count()>0,'Preview must show the unavailable risk form');
  for(const input of await riskInputs.all())assert(await input.isDisabled(),'Preview must never enable the live risk configuration');
  assert.equal(await page.locator('.subpage-secondary .live-indicator strong').first().innerText(),'DESIGN PREVIEW','Preview must not claim MT5 is connected');
  await page.getByRole('button',{name:'عرض LIVE',exact:true}).click();await page.locator('.preview-banner').waitFor({state:'hidden'});await status();
 });
 await check('native viewport has no dashboard overflow or overlapping quality labels',async()=>{
  await navigate('نظرة عامة');
  const metrics=await page.evaluate(()=>{
   const c=document.querySelector('main.content');
   const rows=[...document.querySelectorAll('.quality-row')].map(n=>{
    const row=n.getBoundingClientRect(),label=n.querySelector('span').getBoundingClientRect();
    return {top:row.top,bottom:row.bottom,labelTop:label.top,labelBottom:label.bottom};
   });
   return {width:innerWidth,height:innerHeight,contentWidth:c.clientWidth,scrollWidth:c.scrollWidth,rows};
  });
  report.native_layout=metrics;
  await page.locator('.execution-quality').screenshot({path:path.join(out,'quality-layout.png')});
  assert(metrics.scrollWidth<=metrics.contentWidth+1,'Dashboard extends beyond visible content');
  assert.equal(metrics.rows.length,4);
  for(let i=0;i<metrics.rows.length;i++){
   const r=metrics.rows[i];assert(r.labelTop>=r.top-1&&r.labelBottom<=r.bottom+1,'Label escapes its row');
   if(i)assert(r.top>=metrics.rows[i-1].bottom-1,'Quality rows overlap');
  }
 });
 for(const theme of ['Dark','Smoke','Light'])await check(theme+' theme renders',async()=>{
  await page.getByTitle(theme+' theme',{exact:true}).click();await page.waitForFunction(t=>document.body.dataset.theme===t,theme.toLowerCase());await page.locator('main.content').evaluate(n=>n.scrollTop=0);await page.screenshot({path:path.join(out,'theme-'+theme+'.png')});
 });
 await stopFixture();
 await check('safe full exit releases Core port',exitSafely);
 await check('restart preserves mode, learner and commission with STOPPED default',async()=>{
  await launch();await status();await startFixture();const s=await readSettings();
  assert.equal(s.config.trading.mode,'sniper');assert.equal(s.config.trading.sniper_brain,'learner');assert.equal(s.commission_editor.amount,7);
 });
 await stopFixture();
 await check('final safe full exit',exitSafely);
 await check('native staged restore waits for Core and applies on explicit relaunch',async()=>{
  await launch();await status();
  const backup=await invoke('backup_action',{action:'create'});
  assert.equal(typeof backup.name,'string');
  await assert.rejects(()=>invoke('brain_snapshot_restore',{name:backup.name}),
    'A previously connected account must be reverified before restore');
  await status(); // A definite rejection must release the restart barrier.
  await startFixture(); // Synthetic flat MT5, isolated from any real broker.
  const staged=await invoke('brain_snapshot_restore',{name:backup.name});
  assert.equal(staged.staged,true);
  assert.equal(await portOpen(18741),false,'Restore must wait until the original Core exited');
  await assert.rejects(()=>invoke('get_status'),'Background status must not restart Core after staging');
  await stopFixture();
  await invoke('close_terminal').catch(()=>{});await until(()=>app.exitCode!==null);
  browser=undefined;page=undefined;
  await launch();await startFixture();await status();
  const restored=JSON.parse(fs.readFileSync(path.join(home,'restore.last.json'),'utf8'));
  assert.equal(restored.status,'applied');assert.equal(restored.source,backup.name);
  await stopFixture();await exitSafely();
 });
 assert.deepEqual(report.page_errors,[],'Unhandled renderer errors');
}catch(e){failed=true;report.error=String(e);console.error(e);
 if(page&&!page.isClosed()){
  await page.screenshot({path:path.join(out,'failure.png')}).catch(()=>{});
  report.failure_dom=await page.evaluate(()=>({visibility:document.visibilityState,url:location.href,text:document.body.innerText})).catch(()=>null);
 }
}finally{
 await stopFixture();
 if(app&&app.exitCode===null)spawnSync('taskkill.exe',['/PID',String(app.pid),'/T','/F'],{encoding:'utf8'});
 for(const key of policies.reverse()){
  const cleanup=spawnSync('reg.exe',['delete',key,'/v','BrainTradeNext.exe','/f'],{encoding:'utf8'});
  if(cleanup.status!==0){failed=true;report.policy_cleanup='FAIL'}
 }
 fs.closeSync(log);report.finished_utc=new Date().toISOString();report.status=failed?'FAIL':'PASS';
 fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2)+'\n');
}
process.exitCode=failed?1:0;
