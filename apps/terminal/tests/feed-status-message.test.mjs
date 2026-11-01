import assert from 'node:assert/strict';
import test from 'node:test';
import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {connectionStatusMessage,feedEvidence} from '../src/feed.ts';

const status={
 gateway_fresh:true,
 last_received_ms:9000,
 message:'اتصال MT5 قائم — رسالة محفوظة من ربط سابق',
 telemetry:{connected:true,tick_age_ms:100,server:'Demo',login:1,symbol:'XAUUSD'}
};
const live=feedEvidence(status,true,9500,10000);

test('only quote-fresh broker is allowed to display last Core connection message',()=>{
 assert.equal(live.core,true);
 assert.equal(live.broker,true);
 assert.equal(live.quote,true);
 assert.match(connectionStatusMessage(status,live,false),/اتصال MT5 قائم/);
});

test('stale Core heartbeat never advertises cached MT5 connected text',()=>{
 const evidence=feedEvidence(status,true,1000,10000);
 assert.equal(evidence.core,false);
 assert.doesNotMatch(connectionStatusMessage(status,evidence,false),/اتصال MT5 قائم/);
 assert.match(connectionStatusMessage(status,evidence,false),/Core غير متاح/);
});

test('stale MT5 broker snapshot never advertises old connected text',()=>{
 const expired={...status,gateway_fresh:false};
 const evidence=feedEvidence(expired,true,9500,10000);
 assert.equal(evidence.core,true);
 assert.equal(evidence.broker,false);
 const text=connectionStatusMessage(expired,evidence,false);
 assert.match(text,/اتصال MT5 غير مؤكد/);
 assert.doesNotMatch(text,/اتصال MT5 قائم/);
});

test('connected account with a stale quote is distinguished from trade-ready quote',()=>{
 const old={...status,telemetry:{...status.telemetry,tick_age_ms:4200}};
 const evidence=feedEvidence(old,true,9500,10000);
 assert.equal(evidence.broker,true);
 assert.equal(evidence.quote,false);
 const text=connectionStatusMessage(old,evidence,false);
 assert.match(text,/آخر سعر غير حديث/);
 assert.doesNotMatch(text,/اتصال MT5 قائم/);
});

test('preview is always clearly synthetic, regardless of cached Core evidence',()=>{
 assert.match(connectionStatusMessage(status,live,true),/معاينة التصميم فقط/);
 assert.doesNotMatch(connectionStatusMessage(status,live,true),/اتصال MT5 قائم/);
});

test('dashboard uses evidence-based labels instead of cached status.message',()=>{
 const ui=readFileSync(fileURLToPath(new URL('../src/terminal.tsx',import.meta.url)),'utf8');
 assert.match(ui,/connectionStatusMessage\(status,feed,preview\)/);
 assert.match(ui,/feed\.broker\?'Connected broker':'Not verified'/);
 assert.doesNotMatch(ui,/\{status\?\.message\|\|'بانتظار حالة Rust Core/);
});

test('every secondary page distinguishes Core/broker connectivity from an actual fresh quote',()=>{
 const ui=readFileSync(fileURLToPath(new URL('../src/terminal.tsx',import.meta.url)),'utf8');
 assert.match(ui,/MT5 ACCOUNT CONNECTED • QUOTES STALE/);
 assert.match(ui,/RUST CORE ONLINE • BROKER UNVERIFIED/);
 assert.match(ui,/RUST CORE OFFLINE/);
 assert.match(ui,/<ConnectionLine feed=\{feed\} preview=\{preview\}\/>/);
 assert.doesNotMatch(ui,/ConnectionLine connectedNow=\{trustworthy\}/);
});
