import type {BrokerJournalRow} from './journal-evidence';

const headings=['Position ID','Symbol','Side','Status','Opened UTC','Mode','Brain','Volume','Entry price','Net','Commission','Swap','Fee'];
const values=(r:BrokerJournalRow)=>([String(r.positionId),r.symbol,r.side,r.status,r.opened,r.mode,r.brain,r.lotsOpen,r.entryPrice,r.net,r.commission,r.swap,r.fee]);

// CSV is downloaded locally only after a user click. Neutralize spreadsheet
// formulas in broker-provided strings; never convert missing amounts to zero.
function cell(value:string|number|null):string{
 if(value===null)return '';
 let text=String(value);
 if(typeof value==='string'&&/^[\u0000-\u0020]*[=+\-@]/.test(text))text="'"+text;
 return '"'+text.replace(/"/g,'""')+'"';
}
export function journalCsv(rows:BrokerJournalRow[]):string{
 const data=rows.map(values);
 return '\ufeff'+[headings,...data].map(row=>row.map(cell).join(',')).join('\r\n')+'\r\n';
}

// Minimal, uncompressed ZIP package for an OOXML workbook. The workbook uses
// inline strings, never formula cells, and preserves absent numbers as blanks.
// No broker data leaves the machine. Source rows are already scope-validated.
function xmlText(value:string):string{
 return Array.from(value,ch=>{
  const cp=ch.codePointAt(0)!;
  return cp===9||cp===10||cp===13||(cp>=32&&cp<=0xd7ff)||(cp>=0xe000&&cp<=0xfffd)||(cp>=0x10000&&cp<=0x10ffff)?ch:'\ufffd';
 }).join('').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}
function crc32(bytes:Uint8Array):number{
 let crc=0xffffffff;
 for(const byte of bytes){crc^=byte;for(let bit=0;bit<8;bit++)crc=(crc>>>1)^((crc&1)?0xedb88320:0)}
 return (crc^0xffffffff)>>>0;
}
function storedZip(files:{name:string;data:string}[]):Uint8Array<ArrayBuffer>{
 const encode=new TextEncoder(),local:Uint8Array[]=[],central:Uint8Array[]=[];
 let offset=0;
 const header=(length:number)=>{const b=new Uint8Array(length);return {b,v:new DataView(b.buffer)}};
 for(const file of files){
  const name=encode.encode(file.name),data=encode.encode(file.data),crc=crc32(data);
  const l=header(30);l.v.setUint32(0,0x04034b50,true);l.v.setUint16(4,20,true);l.v.setUint16(6,0x0800,true);
  l.v.setUint16(12,0x21,true);l.v.setUint32(14,crc,true);l.v.setUint32(18,data.length,true);l.v.setUint32(22,data.length,true);l.v.setUint16(26,name.length,true);
  local.push(l.b,name,data);
  const c=header(46);c.v.setUint32(0,0x02014b50,true);c.v.setUint16(4,20,true);c.v.setUint16(6,20,true);
  c.v.setUint16(8,0x0800,true);c.v.setUint16(14,0x21,true);c.v.setUint32(16,crc,true);
  c.v.setUint32(20,data.length,true);c.v.setUint32(24,data.length,true);c.v.setUint16(28,name.length,true);c.v.setUint32(42,offset,true);
  central.push(c.b,name);offset+=l.b.length+name.length+data.length;
 }
 const size=central.reduce((n,b)=>n+b.length,0),end=header(22);
 end.v.setUint32(0,0x06054b50,true);end.v.setUint16(8,files.length,true);end.v.setUint16(10,files.length,true);
 end.v.setUint32(12,size,true);end.v.setUint32(16,offset,true);
 const result=new Uint8Array(offset+size+end.b.length);let cursor=0;
 for(const part of [...local,...central,end.b]){result.set(part,cursor);cursor+=part.length}
 return result;
}
export function journalXlsx(rows:BrokerJournalRow[],scope:string):Uint8Array<ArrayBuffer>{
 const data:(string|number|null)[][]=[[scope],headings,...rows.map(values)];
 const sheetRows=data.map((row,index)=>{
  const cells=row.map((value,col)=>{
   if(value===null||typeof value==='number'&&!Number.isFinite(value))return '';
   const address=String.fromCharCode(65+col)+(index+1);
   return typeof value==='number'?`<c r="${address}"><v>${value}</v></c>`:
    `<c r="${address}" t="inlineStr"><is><t xml:space="preserve">${xmlText(value)}</t></is></c>`;
  }).join('');return `<row r="${index+1}">${cells}</row>`;
 }).join('');
 const prefix='<?xml version="1.0" encoding="UTF-8" standalone="yes"?>';
 return storedZip([
  {name:'[Content_Types].xml',data:prefix+'<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>'},
  {name:'_rels/.rels',data:prefix+'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>'},
  {name:'xl/workbook.xml',data:prefix+'<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Trades" sheetId="1" r:id="rId1"/></sheets></workbook>'},
  {name:'xl/_rels/workbook.xml.rels',data:prefix+'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>'},
  {name:'xl/worksheets/sheet1.xml',data:prefix+'<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetViews><sheetView workbookViewId="0"><pane ySplit="2" topLeftCell="A3" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews><cols><col min="1" max="13" width="20" customWidth="1"/></cols><sheetData>'+sheetRows+'</sheetData></worksheet>'},
 ]);
}
