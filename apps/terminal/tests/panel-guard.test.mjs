import assert from 'node:assert/strict';
import test from 'node:test';
import {PanelReadFence} from '../src/panel-guard.ts';

const deferred=()=>{
 let resolve,reject;
 const promise=new Promise((res,rej)=>{resolve=res;reject=rej;});
 return {promise,resolve,reject};
};
async function modelRead(fence, d, change){
 const ticket=fence.beginRead();
 try{
   const outcome=await d.promise;
   if(fence.isCurrent(ticket))change('success',outcome);
 }catch(error){
   if(fence.isCurrent(ticket))change('error',error.message);
 }
}
test('out-of-order reads on same account/route: older response cannot overwrite',async()=>{
 const f=new PanelReadFence(); f.setScope('brains:account17:demo');
 const old=deferred(),fresh=deferred(),published=[];
 const a=modelRead(f,old,(...args)=>published.push(args));
 const b=modelRead(f,fresh,(...args)=>published.push(args));
 fresh.resolve('new-advisor-state');await b;
 old.resolve('old-advisor-state');await a;
 assert.deepEqual(published,[['success','new-advisor-state']]);
});
test('account mode and page switches invalidate in-flight reads',async()=>{
 const f=new PanelReadFence();f.setScope('risk:17:demo');
 const inflight=deferred(),published=[];
 const a=modelRead(f,inflight,(...args)=>published.push(args));
 f.setScope('risk:17:real');f.setScope('risk:17:demo');
 inflight.resolve('stale');await a;
 assert.deepEqual(published,[],'return to same scope still invalidates earlier request');
});
test('stale errors do not wipe newer valid data',async()=>{
 const f=new PanelReadFence();f.setScope('execution:account17');
 const a=deferred(),b=deferred(),published=[];
 const p1=modelRead(f,a,(...args)=>published.push(args));
 const p2=modelRead(f,b,(...args)=>published.push(args));
 b.resolve('new');await p2;
 a.reject(new Error('old timeout'));await p1;
 assert.deepEqual(published,[['success','new']]);
});
test('unchanged scope retains current completed read without phantom invalidation',async()=>{
 const f=new PanelReadFence();f.setScope('dashboard');
 const serial=f.beginRead();f.setScope('dashboard');
 assert.equal(f.isCurrent(serial),true);
 f.beginRead();assert.equal(f.isCurrent(serial),false);
});
test('invalid revision tokens always fail closed',()=>{
 const f=new PanelReadFence();f.setScope('foo');
 assert.equal(f.isCurrent(NaN),false);
 assert.equal(f.isCurrent(Infinity),false);
 assert.equal(f.isCurrent(0),false);
});
