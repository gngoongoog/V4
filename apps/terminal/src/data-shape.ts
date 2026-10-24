// Bounded, read-only view of arbitrary V4 Core response shapes. No synthetic
// analytics: all rows and their labels come from the selected Core endpoint.
export type CoreRow=Record<string,unknown>;
export type CoreGroup={name:string;rows:CoreRow[]};
const COLLECTIONS=['events','trades','positions','brains','items','results','records','history','decisions','rows','setups','backtests','snapshots','jobs','evaluations','runs','reports','recent_models','recent','groups','sources','entries','opportunities'];
export function coreGroups(source:unknown):CoreGroup[]{
  if(Array.isArray(source))return [{name:'items',rows:normalize(source)}];
  if(source===null||typeof source!=='object')return [];
  const object=source as Record<string,unknown>;
  const groups:CoreGroup[]=[];
  for(const name of COLLECTIONS){if(Array.isArray(object[name]))groups.push({name,rows:normalize(object[name] as unknown[])})}
  if(!groups.length){
    const summary=Object.entries(object).filter(([,v])=>v===null||typeof v!=='object')
      .slice(0,100).map(([field,value])=>({field,value}));
    if(summary.length)groups.push({name:'summary',rows:summary});
  }
  return groups;
}
export function normalize(input:unknown[]):CoreRow[]{
 return input.slice(0,150).map((item,index)=>{
  if(item&&typeof item==='object'&&!Array.isArray(item))return item as CoreRow;
  return {index,value:item};
 });
}
export function searchRows(rows:CoreRow[],term:string):CoreRow[]{
 const query=term.trim().toLocaleLowerCase();
 if(!query)return rows;
 return rows.filter(row=>{
  try{return JSON.stringify(row).toLocaleLowerCase().includes(query)}catch{return false}
 });
}
export function visibleColumns(rows:CoreRow[]):string[]{
 return [...new Set(rows.slice(0,12).flatMap(r=>Object.keys(r)))].slice(0,8);
}
