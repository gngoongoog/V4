// Actual offline browser interactions; no Core, MT5, network or broker orders.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const os=require('node:os');
const {pathToFileURL}=require('node:url');
const {execFileSync}=require('node:child_process');
let chromium;try{({chromium}=require('playwright'))}catch{({chromium}=require('playwright-core'))}
const root=path.resolve(__dirname,'..');
const supplied=process.argv[2];
const channel=process.argv[3];
const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'braintrade-checklist-'));
const html=supplied?path.resolve(supplied):path.join(temporary,'BRAINTRADE_CHECKLIST_AR.html');
if(!supplied)execFileSync(process.env.PYTHON|| (process.platform==='win32'?'python':'python3'),[path.join(root,'scripts/build_operator_checklist.py'),'--output',html],{cwd:root});
const results=[];
async function check(name,fn){await fn();results.push({name,status:'PASS'});console.log('PASS '+name)}
(async()=>{
 const browser=await chromium.launch({headless:true,...(channel?{channel}:{}),args:['--no-sandbox']});
 try{
  const context=await browser.newContext({acceptDownloads:true});
  const page=await context.newPage();const errors=[];const network=[];
  page.on('pageerror',error=>errors.push(String(error)));
  page.on('request',request=>{if(/^https?:/.test(request.url()))network.push(request.url())});
  await page.goto(pathToFileURL(html).href);
  await check('68 untouched cases render with zero results',async()=>{
   assert.equal(await page.locator('.item').count(),68);
   assert.equal(await page.locator('#progress').getAttribute('value'),'0');
  });
  await check('status and Arabic notes survive reload',async()=>{
   await page.locator('#status-A01').selectOption('pass');
   await page.locator('#notes-A01').fill('فك الحزمة نجح');
   await page.locator('#filter').selectOption('untested');
   await page.locator('#search').fill('B06');
   await page.locator('#status-B06').selectOption('fail');
   await page.locator('#filter').selectOption('all');
   await page.locator('#notes-B06').fill('EA ينتظر — الساعة 10:15');
   await page.locator('#export-json').click();
   await page.reload();
   assert.equal(await page.locator('#status-A01').inputValue(),'pass');
   assert.equal(await page.locator('#status-B06').inputValue(),'fail');
   assert.equal(await page.locator('#notes-B06').inputValue(),'EA ينتظر — الساعة 10:15');
   assert.equal(await page.locator('#progress').getAttribute('value'),'2');
  });
  let exported;
  await check('JSON and Markdown export exact user results without certification',async()=>{
   const event=page.waitForEvent('download');await page.locator('#export-json').click();
   const file=await event;exported=JSON.parse(fs.readFileSync(await file.path(),'utf8'));
   assert.equal(exported.results.length,68);assert.equal(exported.ready_for_real,false);
   assert.equal(exported.user_reported,true);assert.equal(exported.results.find(x=>x.id==='B06').status,'fail');
   const next=page.waitForEvent('download');await page.locator('#export-md').click();
   const markdown=fs.readFileSync(await (await next).path(),'utf8');
   assert.match(markdown,/EA ينتظر/);assert.match(markdown,/\| B06 \| فشل/);
  });
  await check('foreign build and duplicate IDs rejected without overwriting results',async()=>{
   for(const mutate of [x=>x.build_id='another-build',x=>x.results[1].id=x.results[0].id]){
    const invalid=structuredClone(exported);mutate(invalid);
    await page.locator('#import-file').setInputFiles({name:'invalid.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(invalid))});
    await page.waitForFunction(()=>document.getElementById('feedback').textContent.includes('مختلفة')||document.getElementById('feedback').textContent.includes('غير صالح'));
    assert.equal(await page.locator('#status-B06').inputValue(),'fail');
   }
  });
  await check('matching imported notes remain inert text',async()=>{
   exported.results[0].notes='<img src=x onerror="window.injected=1">';
   await page.locator('#import-file').setInputFiles({name:'valid.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(exported))});
   await page.waitForFunction(()=>document.getElementById('feedback').textContent.includes('تم استيراد'));
   assert.equal(await page.locator('#notes-A01').inputValue(),exported.results[0].notes);
   assert.equal(await page.evaluate(()=>window.injected),undefined);
   assert.equal(await page.locator('img').count(),0);
  });
  await check('mobile layout has no horizontal overflow',async()=>{
   await page.setViewportSize({width:390,height:844});
   assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth),true);
  });
  await check('confirmed reset clears only current checklist',async()=>{
   page.once('dialog',dialog=>dialog.accept());await page.locator('#reset').click();
   assert.equal(await page.locator('#progress').getAttribute('value'),'0');
  });
  const isolated=await browser.newContext({acceptDownloads:true});
  await isolated.addInitScript(()=>{Object.defineProperty(Storage.prototype,'setItem',{value(){throw new DOMException('disabled','SecurityError')}})});
  const memory=await isolated.newPage();await memory.goto(pathToFileURL(html).href);
  await check('blocked local storage keeps edits and permits explicit export',async()=>{
   await memory.locator('#status-A01').selectOption('pass');
   assert.match(await memory.locator('#feedback').textContent(),/غير متاح/);
   const event=memory.waitForEvent('download');await memory.locator('#export-json').click();
   const data=JSON.parse(fs.readFileSync(await (await event).path(),'utf8'));
   assert.equal(data.results[0].status,'pass');
  });
  assert.deepEqual(errors,[]);assert.deepEqual(network,[]);
  const out=path.join(root,'test-output/operator-checklist');fs.mkdirSync(out,{recursive:true});
  fs.writeFileSync(path.join(out,'report.json'),JSON.stringify({status:'PASS',cases:results,items:68,network_requests:network,page_errors:errors,broker_orders_sent:0},null,2));
  // This screenshot is test evidence only; do not include browser images in
  // a user-facing evidence bundle unless the user separately requests them.
  await memory.screenshot({path:path.join(out,'preview.png'),fullPage:false});
 }finally{await browser.close();fs.rmSync(temporary,{recursive:true,force:true})}
})().catch(error=>{console.error(error);process.exitCode=1});
