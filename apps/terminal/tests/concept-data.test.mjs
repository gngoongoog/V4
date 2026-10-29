import test from 'node:test';
import assert from 'node:assert/strict';
import {reportSeries} from '../src/report-series.ts';
import {journalCsv} from '../src/journal-export.ts';

const row=(id,net,latestDeal='2026-10-28 12:00:00 UTC')=>({
 positionId:id,symbol:'XAUUSD',side:'BUY',status:'CLOSED',opened:'2026-10-28 11:00:00 UTC',
 latestDeal,entryPrice:2000,mode:'sniper',brain:'system',reason:'ok',lotsOpen:.1,lotsClosed:.1,
 remaining:0,profit:net,commission:0,swap:0,fee:0,net,provenCosts:true
});
test('report plots only dated, fully closed and cost-verified broker outcomes in time order',()=>{
 const first=row(1,4,'2026-10-28 12:00:00 UTC'),second=row(2,-1,'2026-10-29 12:00:00 UTC');
 const data=reportSeries([second,{...row(3,99),provenCosts:false},{...row(4,300),status:'OPEN'},
  row(5,200,'not a date'),first]);
 assert.equal(data.length,2);assert.equal(data[0].net,4);assert.equal(data[1].total,3);
 assert.equal(data[1].label,'2026-10-29');
});
test('CSV keeps missing costs blank and neutralizes spreadsheet formulas in text',()=>{
 const data=journalCsv([{...row(7,null),brain:'=HYPERLINK("bad")',mode:'  +cmd',commission:null}]);
 assert(data.startsWith('\ufeff'));assert(data.includes('"\'=HYPERLINK(""bad"")"'));
 assert(data.includes('"\'  +cmd"'));assert(data.includes('"7"'));assert(data.includes(',,'));
});
