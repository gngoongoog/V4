import assert from 'node:assert/strict';
import test from 'node:test';
import {PanelReadFence} from './v25_p0i_panel_guard.ts';

const deferred=()=>{
  let resolve,reject;
  const promise=new Promise((res,rej)=>{resolve=res;reject=rej;});
  return {promise,resolve,reject};
};
async function modelRead(fence, future, publish){
  const ticket=fence.beginRead();
  try {
    const value=await future.promise;
    if(fence.isCurrent(ticket)) publish('success',value);
  } catch(e) {
    if(fence.isCurrent(ticket)) publish('error',String(e.message));
  }
}
test('same page and broker: newest IPC response wins despite reverse delivery',async()=>{
  const fence=new PanelReadFence();fence.setScope('brains:17:Demo');
  const old=deferred(),fresh=deferred(),events=[];
  const p1=modelRead(fence,old,(...v)=>events.push(v));
  const p2=modelRead(fence,fresh,(...v)=>events.push(v));
  fresh.resolve('current');await p2;
  old.resolve('expired');await p1;
  assert.deepEqual(events,[['success','current']]);
});
test('Demo to REAL and back invalidates the original pending request',async()=>{
  const fence=new PanelReadFence();fence.setScope('risk:17:Demo');
  const slow=deferred(),events=[];
  const pending=modelRead(fence,slow,(...v)=>events.push(v));
  fence.setScope('risk:17:REAL');
  fence.setScope('risk:17:Demo');
  slow.resolve('stale');await pending;
  assert.deepEqual(events,[]);
});
test('stale IPC error cannot erase a later successful panel',async()=>{
  const fence=new PanelReadFence();fence.setScope('execution:17');
  const slow=deferred(),fast=deferred(),events=[];
  const p1=modelRead(fence,slow,(...v)=>events.push(v));
  const p2=modelRead(fence,fast,(...v)=>events.push(v));
  fast.resolve('new');await p2;
  slow.reject(new Error('old timeout'));await p1;
  assert.deepEqual(events,[['success','new']]);
});
test('no spurious invalidation if scope is unchanged',()=>{
  const fence=new PanelReadFence();fence.setScope('dashboard');
  const token=fence.beginRead();
  fence.setScope('dashboard');
  assert.equal(fence.isCurrent(token),true);
  fence.beginRead();
  assert.equal(fence.isCurrent(token),false);
});
test('invalid revision tokens fail closed',()=>{
  const fence=new PanelReadFence();fence.setScope('risk');
  assert.equal(fence.isCurrent(NaN),false);
  assert.equal(fence.isCurrent(Infinity),false);
  assert.equal(fence.isCurrent(0),false);
});
