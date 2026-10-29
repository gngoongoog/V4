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
let browser;
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
 const page=await context.newPage(),errors=[];
 page.on('pageerror',e=>errors.push(String(e)));
 await page.goto('http://127.0.0.1:1420/',{waitUntil:'networkidle'});
 await page.locator('#sidebarNav').waitFor();
 assert(await page.locator('.action-btn.start').isDisabled(),'Offline browser cannot start trading');
 await page.getByRole('button',{name:'معاينة التصميم',exact:true}).click();
 assert(await page.locator('.preview-banner').isVisible(),'Synthetic preview must be labelled');
 const primary=[['dashboard','نظرة عامة'],['trading','مركز التداول'],['brainControl','إدارة العقول'],['risk','إدارة المخاطر'],['reports','التقارير'],['journal','السجلات']];
 for(const [id,title] of primary){
  const button=page.locator('#sidebarNav').getByTitle(title,{exact:true});await button.click();
  await page.waitForFunction(title=>document.querySelector('#sidebarNav [aria-current="page"]')?.getAttribute('title')===title,title);
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
 }
 await page.setViewportSize({width:1120,height:730});
 await page.locator('#sidebarNav').getByTitle('نظرة عامة',{exact:true}).click();
 await page.screenshot({path:path.join(out,'dashboard-1120.png')});
 const narrow=await page.locator('main.content').evaluate(e=>({scroll:e.scrollWidth,visible:e.clientWidth}));
 assert(narrow.scroll<=narrow.visible+1,'Laptop dashboard has horizontal overflow');
 assert.deepEqual(errors,[],'Unhandled renderer errors');
 fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({result:'PASS',pages:pages.length,primaryScreens:primary.map(p=>p[0]),viewports:['1536x864','1120x730'],syntheticPreview:true,mt5:'NOT TESTED',orders:'NOT SENT'},null,2));
}finally{
 if(browser)await browser.close();
 server.kill();
}
