// Render the actual React frontend in an isolated browser. No Tauri, MT5 or orders.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {spawn} from 'node:child_process';
import {chromium} from 'playwright-core';

assert.equal(process.env.GITHUB_ACTIONS,'true','This fixture is for hosted CI only');
const root=process.cwd(),out=path.resolve('test-output/concept4-visual');
fs.mkdirSync(out,{recursive:true});
const chrome=['/usr/bin/google-chrome','/usr/bin/google-chrome-stable','/usr/bin/chromium'].find(fs.existsSync);
assert(chrome,'Hosted runner Chrome was not found');
const server=spawn('npm',['--prefix','apps/terminal','run','dev'],{cwd:root,stdio:'ignore'});
let browser,page;
const wait=ms=>new Promise(r=>setTimeout(r,ms));
try{
 let ready=false;
 for(let i=0;i<100;i++){
  try{if((await fetch('http://127.0.0.1:1420/')).ok){ready=true;break}}catch{}
  if(server.exitCode!==null)throw Error('Vite exited before readiness');
  await wait(100);
 }
 assert(ready,'Vite readiness timeout');
 browser=await chromium.launch({executablePath:chrome,headless:true,args:['--no-sandbox']});
 const context=await browser.newContext({viewport:{width:1536,height:864},deviceScaleFactor:1});
 page=await context.newPage();const errors=[];
 page.on('pageerror',e=>errors.push(String(e)));
 await page.goto('http://127.0.0.1:1420/',{waitUntil:'networkidle'});
 await page.locator('#sidebarNav').waitFor();
 assert(await page.locator('.action-btn.start').isDisabled(),'Offline browser cannot start trading');
 // A browser without Tauri automatically opens the labelled design preview.
 // In Tauri the operator explicitly enters it with the footer control.
 const enterPreview=page.getByRole('button',{name:'معاينة التصميم',exact:true});
 if(await enterPreview.count())await enterPreview.click();
 assert(await page.locator('.preview-banner').isVisible(),'Synthetic preview must be labelled');
 for(const frame of ['M1','M5','M15','M30','H1','H4','D1']){
  await page.locator('.chart-panel .timeframes').getByRole('button',{name:frame,exact:true}).click();
  assert.equal(await page.locator('.concept-frame select').inputValue(),frame.toLowerCase(),'Chart and header frame must stay synchronized');
  assert.equal(await page.locator('.chart-frame canvas').getAttribute('aria-label'),`MT5 XAUUSD ${frame.toLowerCase()} candlestick chart`);
 }
 await page.locator('.chart-panel .timeframes').getByRole('button',{name:'M15',exact:true}).click();
 await page.getByRole('button',{name:'تكبير الشارت',exact:true}).click();
 assert(await page.locator('.chart-panel.chart-expanded').isVisible());
 const enlarged=await page.locator('.chart-panel.chart-expanded').boundingBox();
 assert(enlarged&&enlarged.width>1400&&enlarged.height>750,'Expanded chart must use the available window');
 await page.keyboard.press('Escape');
 assert.equal(await page.locator('.chart-panel.chart-expanded').count(),0,'Escape returns the chart to its page');
 const primaryTitles=['نظرة عامة','مركز التداول','إدارة العقول','إدارة المخاطر','التقارير','السجلات','الأخبار','الإعدادات'];
 async function assertPrimaryNavigation(){
  const nav=await page.locator('#sidebarNav').boundingBox();assert(nav);
  for(const title of primaryTitles){
   const box=await page.locator('#sidebarNav').getByTitle(title,{exact:true}).boundingBox();assert(box,title+' has no layout');
   assert(box.y>=nav.y-1&&box.y+box.height<=nav.y+nav.height+1,title+' is clipped in the primary navigation');
  }
 }
 await assertPrimaryNavigation();
 const primary=[['dashboard','نظرة عامة'],['trading','مركز التداول'],['brainControl','إدارة العقول'],['risk','إدارة المخاطر'],['reports','التقارير'],['journal','السجلات']];
 for(const [id,title] of primary){
  const button=page.locator('#sidebarNav').getByTitle(title,{exact:true});await button.click();
  await page.waitForFunction(title=>document.querySelector('#sidebarNav [aria-current="page"]')?.getAttribute('title')===title,title);
  // Nav and panel transitions take ~200 ms; capture the settled UI, not the
  // previous page's selection or a partially faded panel.
  await wait(350);
  if(id==='brainControl'){
   assert.equal(await page.locator('.concept-brain-card').count(),16,'Offline directory must reflect the bundled Core inventory');
   for(const button of await page.locator('.concept-brain-card-actions button').all())assert(await button.isDisabled(),'Offline brain controls must be disabled');
  }
  if(id==='risk'){
   for(const input of await page.locator('.risk-editor input').all())assert(await input.isDisabled(),'Unavailable Core risk controls must be disabled');
  }
  await page.screenshot({path:path.join(out,id+'-1536.png')});
  const width=await page.locator('main.content').evaluate(e=>({scroll:e.scrollWidth,visible:e.clientWidth}));
  assert(width.scroll<=width.visible+1,`${id} content has horizontal overflow`);
 }
 await page.locator('#sidebarNav .all-pages').click();
 const pages=[...fs.readFileSync(path.join(root,'apps/terminal/src/terminal.tsx'),'utf8').matchAll(/\{id:'([^']+)',title:'([^']+)',en:'([^']+)'/g)].map(m=>({id:m[1],title:m[2]}));
 assert.equal(pages.length,22);
 for(const item of pages){
  const button=page.locator('#sidebarNav').getByTitle(item.title,{exact:true});await button.click();
  assert.equal(await button.getAttribute('aria-current'),'page',item.id);
  assert((await page.locator('main.content').innerText()).trim().length>15,'Empty page '+item.id);
  const bounds=await page.locator('main.content').evaluate(e=>({scroll:e.scrollWidth,visible:e.clientWidth}));
  assert(bounds.scroll<=bounds.visible+1,item.id+' has desktop horizontal overflow');
  if(!primary.some(([id])=>id===item.id)){
   await page.locator('main.content').evaluate(e=>e.scrollTop=0);
   await wait(250);
   await page.screenshot({path:path.join(out,'extra-'+item.id+'-1536.png')});
  }
 }
 await page.locator('#sidebarNav .all-pages').click();
 await page.setViewportSize({width:1120,height:730});
 await page.locator('#sidebarNav').getByTitle('نظرة عامة',{exact:true}).click();
 await wait(350);
 await assertPrimaryNavigation();
 await page.screenshot({path:path.join(out,'dashboard-1120.png')});
 const narrow=await page.locator('main.content').evaluate(e=>({scroll:e.scrollWidth,visible:e.clientWidth}));
 assert(narrow.scroll<=narrow.visible+1,'Laptop dashboard has horizontal overflow');
 await page.locator('#sidebarNav .all-pages').click();
 for(const item of pages){
  const button=page.locator('#sidebarNav').getByTitle(item.title,{exact:true});await button.click();
  assert.equal(await button.getAttribute('aria-current'),'page',item.id);
  const bounds=await page.locator('main.content').evaluate(e=>({scroll:e.scrollWidth,visible:e.clientWidth}));
  assert(bounds.scroll<=bounds.visible+1,item.id+' has laptop horizontal overflow');
 }
 assert.deepEqual(errors,[],'Unhandled renderer errors');
 fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({result:'PASS',pages:pages.length,primaryScreens:primary.map(p=>p[0]),allPagesCheckedAtBothViewports:true,viewports:['1536x864','1120x730'],syntheticPreview:true,mt5:'NOT TESTED',orders:'NOT SENT'},null,2));
}catch(e){
 if(page&&!page.isClosed())await page.screenshot({path:path.join(out,'failure.png')}).catch(()=>{});
 fs.writeFileSync(path.join(out,'failure.txt'),String(e));
 throw e;
}finally{
 if(browser)await browser.close();
 server.kill();
}
