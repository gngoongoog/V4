import assert from 'node:assert/strict';
import test from 'node:test';
import {OperatorWriteFence} from '../src/operator-write-fence.ts';

const deferred=()=>{let resolve,reject;const promise=new Promise((ok,bad)=>{resolve=ok;reject=bad});return {promise,resolve,reject}};
async function modelWrite(fence,label,work,observed){
 const ticket=fence.begin(label);if(!ticket)return false;
 try{
  const outcome=await work.promise;
  if(!fence.isCurrent(ticket))return false;
  const next=await Promise.resolve(outcome);
  if(!fence.isCurrent(ticket))return false;
  observed.push(['confirmed',label,next]);
 }catch(e){if(fence.isCurrent(ticket))observed.push(['failed',label,String(e)])}
 finally{fence.finish(ticket)}
 return true;
}

test('same-account write can report Core result exactly once',async()=>{
 const f=new OperatorWriteFence();f.setScope('risk:demo17');
 const work=deferred(),events=[];
 const op=modelWrite(f,'risk',work,events);
 assert.equal(f.busy,true);
 assert.equal(f.begin('settings'),null,'double click and overlapping writes must be rejected synchronously');
 work.resolve({version:2});await op;
 assert.equal(f.busy,false);
 assert.deepEqual(events,[['confirmed','risk',{version:2}]]);
});

test('account A -> B -> A invalidates pending success while serializing next write',async()=>{
 const f=new OperatorWriteFence();f.setScope('settings:A');
 const old=deferred(),events=[];
 const op=modelWrite(f,'config',old,events);
 f.setScope('settings:B');
 assert.equal(f.begin('config'),null,'no overlapping IPC mutations on new account while old request unresolved');
 f.setScope('settings:A');
 old.resolve('old secret persisted');
 await op;
 assert.deepEqual(events,[],'old account success must not be surfaced when returning to account A');
 assert.equal(f.busy,false);
 const latest=deferred();const op2=modelWrite(f,'config',latest,events);
 latest.resolve('new current result');await op2;
 assert.deepEqual(events,[['confirmed','config','new current result']]);
});

test('stale timeout does not overwrite current account notices',async()=>{
 const f=new OperatorWriteFence();f.setScope('brain:17');
 const work=deferred(),events=[];
 const op=modelWrite(f,'brain-control',work,events);
 f.setScope('brain:18');
 work.reject(new Error('unresolved old-account timeout'));
 await op;
 assert.deepEqual(events,[]);
 assert.equal(f.busy,false);
});

test('switching page or preview revokes write evidence but does not pretend to cancel Core request',async()=>{
 const f=new OperatorWriteFence();f.setScope('trading:17:live');
 const promise=deferred(),events=[];
 const op=modelWrite(f,'trading',promise,events);
 f.setScope('dashboard:17:live');
 f.setScope('dashboard:17:preview');
 assert.equal(f.begin('live-binding'),null,'remain busy until Tauri IPC settles');
 promise.resolve({saved:true});await op;
 assert.deepEqual(events,[]);
});

test('tampered ticket and wrong finish cannot release live write',()=>{
 const f=new OperatorWriteFence();f.setScope('risk:A');
 assert.equal(f.begin(''),null);
 const real=f.begin('risk');
 assert(real&&f.busy);
 assert.equal(f.isCurrent({...real}),false,'only exact issued identity may authorize continuation');
 assert.equal(f.isCurrent({...real,epoch:real.epoch+1}),false);
 assert.equal(f.isCurrent({...real,scope:'risk:B'}),false);
 assert.equal(f.finish({...real}),false);
 assert.equal(f.busy,true);
 assert.equal(f.finish(real),true);
 assert.equal(f.busy,false);
 assert.equal(f.finish(real),false);
});

test('ui Core mutations are wired to a fence before sending and checking acknowledgements',()=>{
 const fs=require('node:fs');
 const path=require('node:path');
 const ui=fs.readFileSync(path.resolve(import.meta.dirname,'../src/terminal.tsx'),'utf8');
 const blocks=[
  ['risk','saveRisk'],['portfolio','savePortfolio'],['config','saveSettings'],
  ['handoff','handoff'],['trading','saveTrading'],['brain-control','updateBrainControl'],
  ['broker-binding','bindExecution']
 ];
 for(const [label,action] of blocks){
  const start=ui.indexOf(`const mutation=beginWrite('${label}')`);
  assert(start>=0,'missing serialized '+label);
  const send=ui.indexOf(`await ${action}(`,start);
  assert(send>start,'missing Core IPC write: '+action);
  const checked=ui.indexOf('if(!writeFresh(mutation))return;',send);
  assert(checked>send && checked<send+350,'must fence '+label+' before follow-up read/state');
 }
 assert(ui.includes('mutationFence.current.setScope(viewScope)'));
 assert(ui.includes('if(!fresh)sendNotice('));
});
