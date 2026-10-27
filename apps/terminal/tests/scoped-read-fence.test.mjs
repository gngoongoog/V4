import assert from 'node:assert/strict';
import test from 'node:test';
import {ScopedReadFence} from '../src/scoped-read-fence.ts';

function deferred(){
 let resolve,reject;
 const promise=new Promise((ok,err)=>{resolve=ok;reject=err});
 return {promise,resolve,reject};
}
async function consume(fence,channel,work,seen){
 const ticket=fence.beginRead(channel);
 try{
  const value=await work.promise;
  if(fence.isCurrent(ticket))seen.push([channel,'ok',value]);
 }catch(e){
  if(fence.isCurrent(ticket))seen.push([channel,'err',String(e)]);
 }
}
test('same-account delayed older brain response cannot overwrite newer evidence',async()=>{
 const f=new ScopedReadFence();f.setScope('Demo:17:XAUUSD:sniper');
 const first=deferred(),second=deferred(),seen=[];
 const p1=consume(f,'brains',first,seen),p2=consume(f,'brains',second,seen);
 second.resolve('new');await p2;first.resolve('old');await p1;
 assert.deepEqual(seen,[['brains','ok','new']]);
});
test('risk, learning, config and brain channels are independent',async()=>{
 const f=new ScopedReadFence();f.setScope('Demo:17:XAUUSD:sniper');
 const risk=deferred(),brain=deferred(),newBrain=deferred(),seen=[];
 const a=consume(f,'risk',risk,seen);
 const b=consume(f,'brains',brain,seen);
 const c=consume(f,'brains',newBrain,seen);
 risk.resolve('valid risk');await a;newBrain.resolve('new brain');await c;
 brain.resolve('old brain');await b;
 assert.deepEqual(seen,[['risk','ok','valid risk'],['brains','ok','new brain']]);
});
test('account A -> B -> A rejects prior A response',async()=>{
 const f=new ScopedReadFence();f.setScope('Demo:17:XAUUSD:sniper');
 const early=deferred(),seen=[];
 const request=consume(f,'brains',early,seen);
 f.setScope('Real:18:XAUUSD:sniper');
 f.setScope('Demo:17:XAUUSD:sniper');
 early.resolve('cached account-17 data');await request;
 assert.deepEqual(seen,[]);
});
test('Demo/REAL switch and broker disconnect revoke earlier risk reads',async()=>{
 const f=new ScopedReadFence();f.setScope('17:Demo:online:true');
 const old=deferred(),seen=[];
 const p=consume(f,'risk',old,seen);
 f.setScope('17:Real:online:true');
 f.setScope('17:Real:online:false');
 old.resolve('obsolete risk state');await p;
 assert.deepEqual(seen,[]);
});
test('manual trading mode switch invalidates pending brain evidence only',async()=>{
 const f=new ScopedReadFence();f.setScope('17:Demo:online:true');
 const old=deferred(),learning=deferred(),seen=[];
 const b=consume(f,'brains',old,seen);
 const l=consume(f,'learning',learning,seen);
 f.invalidate('brains');
 old.resolve('old expert mode');await b;
 learning.resolve('learning still current');await l;
 assert.deepEqual(seen,[['learning','ok','learning still current']]);
});
test('late timeout from stale read cannot null-out latest result',async()=>{
 const f=new ScopedReadFence();f.setScope('A');
 const first=deferred(),second=deferred(),seen=[];
 const a=consume(f,'config',first,seen);
 const b=consume(f,'config',second,seen);
 second.resolve({version:2});await b;
 first.reject(new Error('timeout'));await a;
 assert.deepEqual(seen,[['config','ok',{version:2}]]);
});
test('input revision and missing channels fail closed',()=>{
 const f=new ScopedReadFence();f.setScope('A');
 assert.throws(()=>f.beginRead(''),/Missing Core read channel/);
 const ticket=f.beginRead('brains');
 assert.equal(f.isCurrent(ticket),true);
 assert.equal(f.isCurrent({...ticket,revision:NaN}),false);
 assert.equal(f.isCurrent({...ticket,revision:Infinity}),false);
 assert.equal(f.isCurrent({...ticket,channel:'risk'}),false);
 f.invalidate('brains');assert.equal(f.isCurrent(ticket),false);
});
