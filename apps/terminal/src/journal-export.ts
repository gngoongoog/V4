import type {BrokerJournalRow} from './journal-evidence';

// CSV is downloaded locally only after a user click. Neutralize spreadsheet
// formulas in broker-provided strings; never convert missing amounts to zero.
function cell(value:string|number|null):string{
 if(value===null)return '';
 let text=String(value);
 if(typeof value==='string'&&/^[\u0000-\u0020]*[=+\-@]/.test(text))text="'"+text;
 return '"'+text.replace(/"/g,'""')+'"';
}
export function journalCsv(rows:BrokerJournalRow[]):string{
 const headings=['Position ID','Symbol','Side','Status','Opened UTC','Mode','Brain','Volume','Entry price','Net','Commission','Swap','Fee'];
 const data=rows.map(r=>[r.positionId,r.symbol,r.side,r.status,r.opened,r.mode,r.brain,r.lotsOpen,r.entryPrice,r.net,r.commission,r.swap,r.fee]);
 return '\ufeff'+[headings,...data].map(row=>row.map(cell).join(',')).join('\r\n')+'\r\n';
}
