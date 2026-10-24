import type {Status} from './types';

export function quoteClockEvidence(status:Status|null|undefined, preview=false){
 const t=status?.telemetry;
 const empty={source:null as number|null,event:null as number|null,offset:null as number|null};
 if(preview)return {...empty,state:'preview'};
 if(!status?.gateway_fresh||!t?.connected)return {...empty,state:'unavailable'};
 const event=t.quote_time_ms,source=t.source_quote_time_ms;
 const valid=(v:unknown):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>0;
 if(source==null)return {...empty,state:'legacy'};
 if(!valid(event)||!valid(source)||source>event)return {...empty,state:'invalid'};
 return {source,event,offset:event-source,state:'observed'};
}
