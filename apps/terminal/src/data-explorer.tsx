import {useEffect,useMemo,useState} from 'react';
import {Database,FileJson2,Search,RefreshCw,ChevronDown} from 'lucide-react';
import {coreGroups,searchRows,visibleColumns} from './data-shape';
import type {CoreRow} from './data-shape';

function display(value:unknown):string{
 if(value===null||value===undefined)return '—';
 if(typeof value==='number')return Number.isFinite(value)?value.toLocaleString('en-US',{maximumFractionDigits:4}):'—';
 if(typeof value==='boolean')return value?'نعم':'لا';
 if(typeof value==='object'){
   try{return JSON.stringify(value).slice(0,130)}catch{return '[unsupported]'}
 }
 return String(value).slice(0,160);
}
export default function DataExplorer({source,error,preview,updatedAt,onRefresh}: {source:unknown;error:string;preview:boolean;updatedAt:number|null;onRefresh:()=>void}){
 const groups=useMemo(()=>coreGroups(source),[source]);
 const [group,setGroup]=useState('');const [query,setQuery]=useState('');const [chosen,setChosen]=useState<number|null>(null);
 const selected=groups.find(x=>x.name===group)??groups[0];
 const rows=useMemo(()=>searchRows(selected?.rows??[],query),[selected,query]);
 const headers=useMemo(()=>visibleColumns(rows),[rows]);
 useEffect(()=>{setChosen(null)},[group,query,source]);
 const inspected:CoreRow|null=chosen!==null?rows[chosen]??null:null;
 return <section className="core-explorer" aria-label="مستعرض بيانات Rust Core">
  <div className="explorer-toolbar">
   <div className="explorer-source"><Database size={17}/><div><strong>Core Data Explorer</strong><small>{preview?'DESIGN PREVIEW':error?'SOURCE ERROR':updatedAt?`VERIFIED RESPONSE · ${new Date(updatedAt).toLocaleTimeString('en-GB')}`:'AWAITING CORE RESPONSE'}</small></div></div>
   <button type="button" className="explorer-refresh" disabled={preview} onClick={onRefresh}><RefreshCw size={15}/> تحديث المصدر</button>
  </div>
  {preview?<div className="explorer-empty">المعاينة غير متصلة ببيانات العقول أو الصفقات الحقيقية. افتح التطبيق على ويندوز لتصفح بيانات Core.</div>:
   error?<div className="explorer-error" role="alert">تعذر استرجاع بيانات Core: {error}</div>:
   source===null?<div className="explorer-empty">جارِ تحميل البيانات الأصلية من Rust Core...</div>:
   <>
    <div className="explorer-groups">{groups.map(item=><button type="button" key={item.name} className={item.name===selected?.name?'active':''} onClick={()=>{setGroup(item.name);setQuery('')}}>{item.name}<b>{item.rows.length}</b></button>)}{!groups.length&&<span>هذا المصدر لم يُرجع سجلات قابلة للعرض</span>}</div>
    <div className="explorer-filter"><label><Search size={15}/><input aria-label="بحث داخل سجلات Core" type="search" value={query} onChange={e=>setQuery(e.target.value)} placeholder="ابحث في الإشارات، الأوامر، الأحداث، الرموز..."/></label><span>{rows.length} سجل / حد العرض 150</span></div>
    <div className="explorer-table table-scroll">{rows.length?<table><thead><tr>{headers.map(name=><th key={name}>{name}</th>)}<th>التفاصيل</th></tr></thead><tbody>{rows.map((row,i)=><tr key={i} className={chosen===i?'row-selected':''}>{headers.map(h=><td key={h} title={display(row[h])}>{display(row[h])}</td>)}<td><button type="button" className="explorer-detail" onClick={()=>setChosen(chosen===i?null:i)} aria-expanded={chosen===i}><ChevronDown size={14}/>{chosen===i?'إخفاء':'عرض'}</button></td></tr>)}</tbody></table>:<div className="explorer-empty">{query?'لا توجد سجلات مطابقة للبحث':'لا توجد سجلات في هذه المجموعة'}</div>}</div>
    {inspected&&<div className="explorer-inspector"><strong><FileJson2 size={16}/> سجل من Core · قراءة فقط</strong><pre>{JSON.stringify(inspected,null,2).slice(0,10000)}</pre></div>}
   </>}
 </section>
}
