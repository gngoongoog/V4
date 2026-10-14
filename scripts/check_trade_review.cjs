// Run after smoke_core.py. Input exports are synthetic and offline only.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {pathToFileURL}=require('node:url');
const {chromium}=require('playwright');
(async()=>{
 const browser=await chromium.launch({headless:true});
 try {
  for(const width of [1200,880]) {
   const page=await browser.newPage({viewport:{width,height:900}});
   const errors=[],network=[];
   page.on('pageerror',e=>errors.push(e.message));
   page.on('request',r=>{if(/^https?:/.test(r.url()))network.push(r.url());});
   for(const name of ['kte-report','meta-report','empty-report','meta-v2-report']) {
    const folder=path.resolve(__dirname,'../test-output',name);
    const report=JSON.parse(fs.readFileSync(path.join(folder,'report.json'),'utf8'));
    await page.goto(pathToFileURL(path.join(folder,'report.html')).href);
    const select=page.locator('#trade-select');
    assert.equal(await select.locator('option').count(),report.trades.length);
    if(!report.trades.length) {
     assert(await select.isDisabled());assert(await page.locator('#trade-next').isDisabled());assert(await page.locator('#trade-prev').isDisabled());
     assert.equal(await page.locator('#trade-chart [data-candle]').count(),0);
     assert(await page.locator('#trade-chart').isHidden());
    }else{
     const indexes=[0,report.trades.length-1];
     for(const i of indexes) {
      await select.selectOption(String(i));
      const t=report.trades[i],fills=t.fills?.length?t.fills:[t];
      assert.equal(await page.locator('#trade-fills tr').count(),fills.length);
      assert.equal(await page.locator('#trade-chart [data-fill]').count(),fills.length);
      assert.equal(await page.locator('#trade-chart [data-target]').count(),t.targets?.length||1);
      assert.equal(await page.locator('#trade-chart [data-stop]').count(),t.stop_updates?.length||1);
      assert(await page.locator('#trade-chart [data-candle]').count()>0);
      assert((await page.locator('#trade-detail').innerText()).includes(t.net.toFixed(2)));
      assert(!(await page.locator('#trade-chart').innerHTML()).match(/NaN|Infinity/));
     }
     if(report.trades.length>1) {
      await select.selectOption('0');await page.locator('#trade-next').click();assert.equal(await select.inputValue(),'1');
      await page.locator('#trade-prev').click();assert.equal(await select.inputValue(),'0');
     }
     await select.selectOption('0');
    }
    if(name==='meta-v2-report') {
     assert.equal(await page.locator('#experiment-comparison [data-comparison-metric]').count(),8);
     assert.equal(await page.locator('#candidate-audit [data-candidate-time]').count(),Math.min(50,report.decisions.length));
     const comparison=JSON.parse(fs.readFileSync(path.join(folder,'comparison.json'),'utf8'));
     assert((await page.locator('#experiment-comparison [data-comparison-metric="net"]').innerText()).includes(comparison.target_metrics.net.toFixed(2)));
     await page.locator('#experiment-comparison').screenshot({path:path.resolve(__dirname,`../test-output/comparison-${width}.png`)});
     await page.locator('#candidate-audit').screenshot({path:path.resolve(__dirname,`../test-output/audit-${width}.png`)});
    }
    assert.deepEqual(errors,[]);assert.deepEqual(network,[],'Export must work without remote requests');
    await page.locator('#trade-review').screenshot({path:path.resolve(__dirname,`../test-output/review-${name}-${width}.png`)});
   }
   await page.close();
  }
  console.log('PASS: offline candlestick chart, saved fills/stops/targets, selection, legacy fallback, empty state, 1200/880 layouts');
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
