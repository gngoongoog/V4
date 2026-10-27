import test from 'node:test';
import assert from 'node:assert/strict';
import {quoteClockEvidence} from '../src/quote-clock.ts';

const status={gateway_fresh:true,telemetry:{connected:true,quote_time_ms:5005,source_quote_time_ms:1000}};
test('source time and logical event offset are distinct, never called latency',()=>{
 assert.deepEqual(quoteClockEvidence(status),{state:'observed',source:1000,event:5005,offset:4005});
});
test('preview and stale sessions cannot display previous account timestamps',()=>{
 for(const [s,preview] of [[{...status,gateway_fresh:false},false],[status,true],
  [{...status,telemetry:{...status.telemetry,connected:false}},false],[null,false]]){
  assert.equal(quoteClockEvidence(s,preview).source,null);
  assert.equal(quoteClockEvidence(s,preview).event,null);
 }
});
test('missing and malformed provenance never fabricates a broker timestamp',()=>{
 for(const source of [undefined,0,-1,NaN,Infinity,6000,'1000']){
  assert.equal(quoteClockEvidence({...status,telemetry:{...status.telemetry,source_quote_time_ms:source}}).source,null);
 }
 assert.equal(quoteClockEvidence({...status,telemetry:{...status.telemetry,source_quote_time_ms:null}}).state,'legacy');
});
