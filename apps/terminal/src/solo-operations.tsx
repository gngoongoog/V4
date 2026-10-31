import {useState} from 'react';
import type {Status} from './types';
import {BRAIN_DIRECTORY} from './brain-catalog';

const object=(value:unknown):Record<string,unknown>=>value&&typeof value==='object'&&!Array.isArray(value)?value as Record<string,unknown>:{};

export function SoloOperations({status,source,disabled,armed,onSet}:{
 status:Status|null;source:unknown;disabled:boolean;armed:boolean;onSet:(brain:string)=>Promise<void>;
}){
 const [choice,setChoice]=useState('');
 const view=object(source),settings=object(view.settings),t=status?.telemetry;
 const mode=String(view.trading_mode||'');
 const brains=Array.isArray(view.brains)?view.brains.map(object).map(b=>String(object(b.config).id||''))
  .filter(id=>id!=='system'&&(id!=='sniper_learner'||mode==='sniper')&&BRAIN_DIRECTORY.some(item=>item[0]===id)):[];
 const owner=typeof view.selected_owner==='string'?view.selected_owner:'';
 const available=!!t&&status?.gateway_fresh===true&&t.connected&&view.controls_available===true&&view.account_matches===true&&
  view.context===JSON.stringify([t.server,t.login,t.symbol,t.currency])&&settings.login===t.login&&settings.server===t.server&&settings.currency===t.currency&&
  settings.target===(t.demo?'demo':'live')&&t.account_trade_mode===(t.demo?0:2)&&
  ['sniper','scalping','swing','expert'].includes(mode)&&brains.length>0&&(owner==='system'||brains.includes(owner));
 const selected=brains.includes(choice)?choice:(owner!=='system'&&brains.includes(owner)?owner:brains[0]||'');
 const name=(id:string)=>BRAIN_DIRECTORY.find(item=>item[0]===id)?.[1]||id;
 return <div className="concept-solo" aria-label="اختيار العقل المنفّذ">
  <span>المنفّذ: {available?(owner==='system'?'منسق الوضع':name(owner)):'غير مؤكّد'}</span>
  <select aria-label="العقل للتداول المنفرد" value={selected} disabled={disabled||armed||!available} onChange={e=>setChoice(e.target.value)}>
   {brains.map(id=><option key={id} value={id}>{name(id)}</option>)}
  </select>
  <button type="button" disabled={disabled||armed||!available||!selected||selected===owner} onClick={()=>void onSet(selected)}>تداول بعقل واحد</button>
  <button type="button" disabled={disabled||armed||!available||owner==='system'} onClick={()=>void onSet('system')}>العودة للمنسق</button>
 </div>;
}
