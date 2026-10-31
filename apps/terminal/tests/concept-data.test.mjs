import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {reportSeries} from '../src/report-series.ts';
import {journalCsv,journalXlsx} from '../src/journal-export.ts';
import {BRAIN_DIRECTORY} from '../src/brain-catalog.ts';
import {chartHistory} from '../src/chart-history.ts';

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
test('XLSX exports a scoped workbook with literal text and missing costs blank',()=>{
 const bytes=Buffer.from(journalXlsx([{...row(9007199254740991,null),brain:'=HYPERLINK("bad")',mode:'<test>',commission:null}],
  'حساب تجريبي & النطاق المعروض'));
 const files=new Map();let cursor=0;
 while(bytes.readUInt32LE(cursor)===0x04034b50){
  assert.equal(bytes.readUInt16LE(cursor+8),0,'Workbook entries use a supported ZIP method');
  const size=bytes.readUInt32LE(cursor+18),nameSize=bytes.readUInt16LE(cursor+26),extra=bytes.readUInt16LE(cursor+28);
  const name=bytes.subarray(cursor+30,cursor+30+nameSize).toString('utf8');
  const start=cursor+30+nameSize+extra;files.set(name,bytes.subarray(start,start+size).toString('utf8'));
  cursor=start+size;
 }
 assert.equal(files.size,5);assert.equal(bytes.readUInt32LE(cursor),0x02014b50);
 assert.equal(bytes.readUInt32LE(bytes.length-22),0x06054b50,'ZIP must contain the central directory');
 assert(files.has('xl/workbook.xml')&&files.has('[Content_Types].xml'));
 const sheet=files.get('xl/worksheets/sheet1.xml');
 assert(sheet.includes('حساب تجريبي &amp; النطاق المعروض'));
 assert(sheet.includes('<c r="A3" t="inlineStr"><is><t xml:space="preserve">9007199254740991'));
 assert(sheet.includes('<c r="G3" t="inlineStr"><is><t xml:space="preserve">=HYPERLINK("bad")'));
 assert(sheet.includes('&lt;test&gt;')&&!sheet.includes('<f>'));
 assert(!sheet.includes('<c r="J3"')&&!sheet.includes('<c r="K3"'));
 assert(sheet.includes('<c r="L3"><v>0</v></c>'));
});
test('offline brain directory lists exactly the secondary identities bundled in Core',()=>{
 const domain=readFileSync(new URL('../../../crates/domain/src/execution_legacy.rs',import.meta.url),'utf8');
 const inventory=domain.match(/pub const BRAINS[^=]*=\s*\[([\s\S]*?)\];/);
 assert(inventory,'Core brain inventory was not found');
 const ids=[...inventory[1].matchAll(/\("([a-z_]+)",/g)].map(m=>m[1]).filter(id=>id!=='system').sort();
 assert.deepEqual(BRAIN_DIRECTORY.map(item=>item[0]).sort(),ids);
});
test('extra chart candles are scoped to the current account and validated before display',()=>{
 const now=1_800_000_000_000, account={connected:true,login:7,server:'Demo',symbol:'XAUUSD',currency:'USD',account_trade_mode:0};
 const bars=[1,2,3].map(i=>({time:Math.floor(now/1000)-i*3600,open:100,high:102,low:99,close:101,tick_volume:12})).reverse();
 const row={schema:'chart-frames-v1',ready:true,read_only:true,account:{login:7,server:'Demo',symbol:'XAUUSD',currency:'USD',trade_mode:0},frames:{h1:{bars,captured_ms:now,live:true}}};
 assert.equal(chartHistory(row,account,now).market.h1.length,3);
 assert.equal(chartHistory({...row,frames:{h1:{bars,captured_ms:now-121000,live:false}}},account,now).meta.h1.live,false);
 assert.equal(chartHistory({...row,frames:{h1:{bars,captured_ms:now-121000,live:true}}},account,now).meta.h1.live,false,'Cached live flags must expire');
 assert.deepEqual(chartHistory(row,{...account,login:8},now).market,{});
 assert.deepEqual(chartHistory(row,{...account,account_trade_mode:2},now).market,{});
 assert.deepEqual(chartHistory({...row,frames:{h1:{bars:[...bars].reverse(),captured_ms:now,live:true}}},account,now).market,{});
 assert.deepEqual(chartHistory({...row,frames:{h1:{bars:[{...bars[0],low:104},...bars.slice(1)],captured_ms:now,live:true}}},account,now).market,{});
 assert.deepEqual(chartHistory({...row,frames:{h1:{bars,captured_ms:now+3000,live:true}}},account,now).market,{});
 // The raw history payload must not smuggle an active H1 candle into a
 // broker-closed series. The Rust source is already stricter; this is a
 // separate renderer trust boundary with its own regression.
 const unfinished=[...bars.slice(0,2),{...bars[2],time:Math.floor(now/1000)-1800}];
 assert.deepEqual(chartHistory({...row,frames:{h1:{bars:unfinished,captured_ms:now,live:true}}},account,now).market,{},
   'Never display an H1 forming candle as already closed');
});
