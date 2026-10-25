// Real packaged Tauri/WebView2 only. Never substitutes browser fixtures or MT5.
import {chromium} from 'playwright-core';
import {spawn,execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import fs from 'node:fs/promises';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
import {inspect} from 'node:util';
import {fileURLToPath} from 'node:url';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const args=process.argv.slice(2);
const option=(key)=>args[args.indexOf(key)+1];
if(process.platform!=='win32'||!args.includes('--exe')||!args.includes('--expected-source'))
 throw new Error('Requires Windows, --exe actual Portable/BrainTradeNext.exe and --expected-source SHA');
const exe=path.resolve(option('--exe'));
const expected=option('--expected-source');
const output=path.resolve(args.includes('--output')?option('--output'):path.join(root,'test-output/v25/g3b'));
await fs.mkdir(output,{recursive:true});
const report={schema:'braintrade-g3b-webview2-v1',status:'RUNNING',expected_source:expected,
 harness_commit:process.env.GITHUB_SHA??null,real_tauri:true,mocked_ipc:false,
 broker_demo:'NOT TESTED',all_controls_certified:false,ready_for_real:false,
 checks:[],pages:[],console_errors:[],limits:[
 'Offline isolated Windows runner; no MT5, broker orders, EA, real account or profit evidence.',
 '22-page rendering and selected control actions only; connected broker and every control state remain untested.',
 'Screenshots are test fixtures with empty local data, not the user account.']};
const writeReport=()=>fs.writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2)+'\n');
const pause=(ms)=>new Promise(r=>setTimeout(r,ms));
async function until(fn,timeout=20000){const start=Date.now();let error;
 while(Date.now()-start<timeout){try{const v=await fn();if(v)return v;}catch(e){error=e;}await pause(150);}
 throw error??new Error('Timed out waiting for observable state');}
async function freePort(port=0){return new Promise((resolve,reject)=>{
 const server=net.createServer();server.once('error',reject);
 server.listen(port,'127.0.0.1',()=>{const p=server.address().port;server.close(()=>resolve(p));});});}
async function check(name,fn){try{await fn();report.checks.push({name,status:'PASS'});}
 catch(e){report.checks.push({name,status:'FAIL',error:String(e)});throw e;}finally{await writeReport();}}
let child,browser,page,home,appLogs='',machinePolicyOwned=false;
const machinePolicyKey='HKLM\\Software\\Policies\\Microsoft\\Edge\\WebView2\\AdditionalBrowserArguments';
function configureCdpPolicy(port){
 // Current elevated GitHub Windows runners ignore user-scoped WebView2 flags.
 // Scope the documented machine override to this executable on disposable CI only.
 if(process.env.G3B_MACHINE_POLICY!=='1')return;
 assert.equal(process.env.GITHUB_ACTIONS,'true');
 assert.equal(process.env.RUNNER_ENVIRONMENT,'github-hosted');
 if(!machinePolicyOwned){
  let exists=false;
  try{execFileSync('reg',['query',machinePolicyKey,'/v','BrainTradeNext.exe','/reg:64'],{stdio:'pipe'});exists=true;}catch{}
  assert.equal(exists,false,'Refusing to replace an existing WebView2 machine policy');
 }
 execFileSync('reg',['add',machinePolicyKey,'/v','BrainTradeNext.exe','/t','REG_SZ','/d',
  `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1 --disable-gpu`,'/f','/reg:64'],{stdio:'pipe'});
 machinePolicyOwned=true;
 report.cdp_configuration='Temporary per-executable HKLM policy on disposable GitHub Windows runner';
}
async function ipc(command,args={}){
 return page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args});}
async function launch(){
 const port=await freePort();
 configureCdpPolicy(port);
 child=spawn(exe,[],{cwd:home,windowsHide:false,env:{...process.env,
  BRAINTRADE_DATA_DIR:path.join(home,'data'),BRAINTRADE_GATEWAY_DISABLED:'1',BRAINTRADE_NEWS_OFFLINE:'1',
  WEBVIEW2_USER_DATA_FOLDER:path.join(home,'webview'),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1 --disable-gpu`},
  stdio:['ignore','pipe','pipe']});
 child.on('error',e=>{appLogs+=String(e)+'\n';});
 for(const stream of [child.stdout,child.stderr])stream.on('data',b=>{appLogs=(appLogs+b).slice(-100000);});
 const endpoint=await until(async()=>{
  if(child.exitCode!==null)throw new Error('Tauri exited early: '+appLogs);
  const r=await fetch(`http://127.0.0.1:${port}/json/version`,{signal:AbortSignal.timeout(1000)});
  return r.ok?(await r.json()).webSocketDebuggerUrl:false;
 },60000).catch(error=>{throw new Error('WebView2 CDP unavailable: '+inspect(error,{depth:3}));});
 browser=await chromium.connectOverCDP(endpoint);
 page=await until(()=>browser.contexts()[0]?.pages().find(p=>!p.url().startsWith('devtools:')));
 page.on('pageerror',e=>report.console_errors.push(String(e)));
 await page.locator('#sidebarNav').waitFor({state:'visible',timeout:30000});
 // Audit native IPC without replacing its effects or injecting fake responses.
 await page.evaluate(()=>{
  window.__g3ObservedIpc=[];
  const original=window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke=function(command,args,...rest){
   const item={command,route:args?.route??null,settled:false,ok:false};
   window.__g3ObservedIpc.push(item);
   if(window.__g3ObservedIpc.length>3000)window.__g3ObservedIpc.shift();
   return original.call(this,command,args,...rest).then(v=>{item.settled=true;item.ok=true;return v;},
    e=>{item.settled=true;throw e;});
  };
 });
 const status=await ipc('get_status');
 assert.equal(status.requested_run,false);
 assert.equal(status.execution_enabled,false);
 return status;
}
async function navigate(title){
 const expand=page.locator('#sidebarNav .all-pages');
 if(await expand.getAttribute('aria-expanded')!=='true')await expand.click();
 const button=page.locator('#sidebarNav button').filter({hasText:title}).filter({has:page.locator('span')});
 // Exact title avoids fuzzy Arabic text collisions.
 await page.locator(`#sidebarNav button[title="${title}"]`).click();
 await until(async()=>await page.locator(`#sidebarNav button[title="${title}"]`).getAttribute('aria-current')==='page');
}
async function stopAndExit(){
 await navigate('نظرة عامة');
 await page.getByRole('button',{name:'خروج كامل آمن',exact:true}).click();
 const dialog=page.getByRole('dialog',{name:'خروج كامل آمن',exact:true});
 await dialog.getByRole('button',{name:'طلب الخروج الآمن',exact:true}).click();
 await until(()=>child.exitCode!==null,20000);
 assert.equal(child.exitCode,0);
 await browser.close().catch(()=>{});browser=null;page=null;
 await until(async()=>{try{await freePort(18741);return true;}catch{return false;}});
}
try{
 await check('portable_source_and_fingerprint',async()=>{
  assert.match(expected,/^[a-f0-9]{40}$/);
  const build=JSON.parse(await fs.readFile(path.join(path.dirname(exe),'release-evidence/build.json'),'utf8'));
  assert.equal(build.source_commit,expected);
  const bytes=await fs.readFile(exe);assert.equal(bytes.subarray(0,2).toString(),'MZ');
  report.exe_sha256=createHash('sha256').update(bytes).digest('hex');
  assert.equal(report.exe_sha256,build.exe_sha256);report.build=build;
  // Refuse to touch an already running user's Core before creating any process.
  await freePort(18741);
 });
 home=await fs.mkdtemp(path.join(os.tmpdir(),'braintrade-g3b-'));
 await check('launch_real_tauri_stopped_no_broker',launch);
 const source=await fs.readFile(path.join(root,'apps/terminal/src/terminal.tsx'),'utf8');
 const registry=source.slice(source.indexOf('const PAGES:'),source.indexOf('];',source.indexOf('const PAGES:')));
 const pages=[...registry.matchAll(/\{id:'([^']+)',title:'([^']+)'[^\n]+/g)].map(m=>({id:m[1],title:m[2],route:m[0].match(/route:'([^']+)'/)?.[1]}));
 assert.equal(pages.length,22);assert.equal(new Set(pages.map(p=>p.id)).size,22);
 await check('all_22_pages_native_ipc_and_render',async()=>{
  for(const item of pages){
   await page.evaluate(()=>window.__g3ObservedIpc=[]);
   await navigate(item.title);
   if(item.route){await until(()=>page.evaluate(route=>window.__g3ObservedIpc.some(x=>
    x.command==='read_panel'&&x.route===route&&x.settled&&x.ok),item.route));}
   assert.ok((await page.locator('main').innerText()).trim().length>10);
   const controls=await page.locator('main button,main input,main select,main textarea').evaluateAll(nodes=>nodes.map(n=>({
    tag:n.tagName,type:n.getAttribute('type'),name:n.getAttribute('aria-label')||n.textContent?.trim().slice(0,120),
    disabled:n.disabled===true,action_tested:false})));
   await page.screenshot({path:path.join(output,`${item.id}.png`),fullPage:true});
   report.pages.push({...item,status:'PASS',controls});await writeReport();
  }
 });
 await check('smoke_dark_light_actual_webview_render',async()=>{
  await navigate('نظرة عامة');
  for(const theme of ['dark','smoke','light']){
   await page.getByTitle(`${theme[0].toUpperCase()+theme.slice(1)} theme`,{exact:true}).click();
   await until(()=>page.locator('body').getAttribute('data-theme').then(v=>v===theme));
   await page.screenshot({path:path.join(output,`dashboard-${theme}.png`),fullPage:true});
  }
  await page.getByTitle('Smoke theme',{exact:true}).click();
 });
 await check('stop_dialog_escape_focus_and_core_effect',async()=>{
  const stop=page.locator('.action-btn.stop');await stop.click();
  const dialog=page.getByRole('dialog',{name:'تأكيد وقف الدخول',exact:true});await dialog.waitFor();
  await page.keyboard.press('Escape');await dialog.waitFor({state:'hidden'});
  assert.equal(await stop.evaluate(e=>e===document.activeElement),true);
  await stop.click();await dialog.getByRole('button',{name:'تأكيد وقف الدخول الجديد',exact:true}).click();
  await dialog.waitFor({state:'hidden'});assert.equal((await ipc('get_status')).requested_run,false);
  assert.equal(await page.locator('.action-btn.start').isDisabled(),true);
 });
 await check('preview_isolation_and_return_to_live',async()=>{
  await page.getByRole('button',{name:'معاينة التصميم',exact:true}).click();
  await page.locator('.preview-banner').waitFor();
  assert.equal(await page.locator('.action-btn.start').isDisabled(),true);
  assert.equal(await page.locator('.action-btn.stop').isDisabled(),true);
  await page.getByRole('button',{name:'عرض LIVE',exact:true}).click();
  await page.locator('.preview-banner').waitFor({state:'hidden'});
  assert.equal((await ipc('get_status')).requested_run,false);
 });
 await check('four_mode_save_commission_and_sniper_controls',async()=>{
  await navigate('مركز التداول');
  const form=page.getByRole('region',{name:'Coordinator and costs'});
  await form.getByRole('button',{name:'حفظ الوضع والعمولة لدى Core',exact:true}).waitFor();
  for(const mode of ['scalping','swing','expert','sniper']){
   const priority=form.locator('input[type="checkbox"]').first();if(await priority.isChecked())await priority.uncheck();
   await form.locator('select').nth(0).selectOption(mode);
   await form.getByLabel('commission per lot',{exact:true}).fill(mode==='scalping'?'0':'7');
   await form.getByLabel('commission basis',{exact:true}).selectOption('round_trip');
   if(mode==='sniper'){
    await form.locator('select').nth(2).selectOption('confirmed');
    await form.locator('select').nth(3).selectOption('learner');
   }
   await form.getByRole('button',{name:'حفظ الوضع والعمولة لدى Core',exact:true}).click();
   await until(async()=>{const c=(await ipc('read_panel',{route:'/v1/config'})).config;
    return c.trading.mode===mode&&c.risk.commission_per_lot===(mode==='scalping'?0:7);});
  }
  const saved=(await ipc('read_panel',{route:'/v1/config'})).config;
  assert.equal(saved.trading.sniper_style,'confirmed');assert.equal(saved.trading.sniper_brain,'learner');
  await form.getByLabel('commission per lot',{exact:true}).fill('99');
  await form.getByRole('button',{name:'التراجع عن التعديلات',exact:true}).click();
  assert.equal(await form.getByLabel('commission per lot',{exact:true}).inputValue(),'7');
 });
 await check('native_read_allowlist_rejects_arbitrary_route',async()=>{
  await assert.rejects(()=>ipc('read_panel',{route:'/v1/shutdown'}));
  assert.equal((await ipc('get_status')).requested_run,false);
 });
 await check('safe_ui_exit_and_persisted_restart',async()=>{
  await stopAndExit();await launch();
  const config=(await ipc('read_panel',{route:'/v1/config'})).config;
  assert.equal(config.trading.mode,'sniper');assert.equal(config.risk.commission_per_lot,7);
  assert.equal(config.trading.sniper_brain,'learner');assert.equal(config.trading.sniper_style,'confirmed');
  await until(()=>page.locator('body').getAttribute('data-theme').then(t=>t==='smoke'));
  await stopAndExit();
 });
 assert.deepEqual(report.console_errors,[],'WebView2 runtime exception');
 report.status='PASS';
}catch(error){report.status='FAIL';report.error=String(error);
 try{
  const diagnostics=execFileSync('powershell',['-NoProfile','-Command',
   "Get-CimInstance Win32_Process | Where-Object { $_.Name -match 'BrainTradeNext|msedgewebview2' } | Select-Object Name,ProcessId,ParentProcessId,CommandLine | ConvertTo-Json -Depth 3"],{encoding:'utf8'});
  await fs.writeFile(path.join(output,'windows-processes.json'),diagnostics);
 }catch{}
 if(page)await page.screenshot({path:path.join(output,'failure.png'),fullPage:true}).catch(()=>{});
 process.exitCode=1;
}finally{
 // Only own isolated process tree; never a process-name/global taskkill.
 if(page)await ipc('shutdown_core').catch(()=>{});
 if(child&&child.exitCode===null){try{execFileSync('taskkill',['/PID',String(child.pid),'/T','/F'],{stdio:'ignore'});}catch{}}
 if(browser)await browser.close().catch(()=>{});
 if(machinePolicyOwned){
  try{execFileSync('reg',['delete',machinePolicyKey,'/v','BrainTradeNext.exe','/f','/reg:64'],{stdio:'pipe'});}
  catch(error){report.status='FAIL';report.policy_cleanup_error=String(error);process.exitCode=1;}
 }
 if(home){await fs.copyFile(path.join(home,'data/core.log'),path.join(output,'core.log')).catch(()=>{});}
 await fs.writeFile(path.join(output,'launcher.log'),appLogs);
 await writeReport();console.log(JSON.stringify({status:report.status,checks:report.checks,pages:report.pages.length}));
}
