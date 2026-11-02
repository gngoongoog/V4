// Executable integration-contract checks for the bounded HVN research path.
const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path');
const root=path.resolve(__dirname,'../../..');
const file=name=>fs.readFileSync(path.join(root,name),'utf8');
const core=file('crates/runtime/src/lib.rs');
const hvn=file('crates/runtime/src/hvn.rs');
const py=file('gateway/hvn_research.py');
const gateway=file('gateway/mt5_gateway.py');
const tauri=file('apps/tauri/src/bridge.rs');
const api=file('apps/terminal/src/bridge.ts');
const ui=file('apps/terminal/src/terminal.tsx');
const workspace=file('apps/terminal/src/hvn-workspace.tsx');
assert.match(core,/mod hvn;/);
assert.match(core,/self\.hvn\.route\(method,path,body,&status,now_ms\(\)\)/);
assert.match(core,/!self\.hvn\.pending\(now_ms\(\)\)/,'Cannot START while MT5 tick-history read is active');
assert.match(core,/ensure_hvn_gateway_reconfiguration_idle\(self\.hvn\.pending\(now_ms\(\)\)\)/,
  'Do not destroy Gateway with an unresolved HVN tick-history request');
assert.match(core,/!self\.backtests\.loading\(\)/,
  'No HVN tick-history read while Backtest is importing MT5 history');
assert.match(core,/method=="POST" && path=="\/v1\/backtest\/start"/,
  'Backtest starts must recognize a pending HVN research read');
assert.match(core,/انتظر تسوية قراءة HVN قبل بدء جلب تاريخ Backtest/,
  'Backtest must not admit a competing MT5 history read');
assert.match(core,/"\/v1\/config"\|"\/v1\/reconnect"\|"\/v1\/backup\/restore\/stage"/,
  'Config, reconnect and staged restore must all be fenced');
assert.match(hvn,/ensure!\(!s\.requested_run/);
assert.match(hvn,/t\.positions\.is_empty\(\) && t\.pending_orders==0/);
assert.match(hvn,/trade_authorized.*false/s);
assert.match(hvn,/report\["observed_to_ms"\]/);
assert.match(hvn,/"report_age_ms":age_ms,"report_stale":stale/,'Core must expose actual observed tick age');
assert.match(py,/"trade_events_observed": len\(trades\)/,'Trade counts must remain distinct');
assert.match(py,/"quote_events_observed": len\(quotes\)/,'Quote counts must remain distinct');
assert.match(py,/if quote_change:/,'Mixed trade-and-quote ticks must not lose quote updates');
assert.doesNotMatch(py,/elif quote_change:/,'Quote changes must be counted independently');
assert.match(hvn,/HVN selected observations disagree with source-specific evidence/,
  'Core must reject inconsistent source-specific counts');
assert.match(hvn,/HVN node activity\/share cannot be reconciled with source total/,
  'Core must reject fabricated HVN node shares');
assert.match(workspace,/report\.trade_events_observed/);
assert.match(workspace,/report\.quote_events_observed/);
assert.match(workspace,/reportStale&&<p role="status"/,'Outdated source must be visibly identified');
assert.match(workspace,/pending&&<p role="status"/,'In-progress read must distinguish older cached research');
assert.match(hvn,/ctx\.as_ref\(\)!=Some\(&pending\.context\)/,'Wrong-account HVN result must be discarded, never credited');
assert.match(py,/def collect_mt5_hvn\(/);
assert.match(py,/copy_ticks_range\(symbol, start, end, mt5\.COPY_TICKS_ALL\)/);
assert.match(py,/if not same_account\(\)/);
assert.match(py,/not isinstance\(raw_flags, Integral\)/,
  'Broker volume requires an authentic integer MT5 change-flag source');
assert.match(py,/trade_change = bool\(flags & \(8 \| 16 \| 32 \| 64\)\)/);
assert.doesNotMatch(py,/order_send\(/);
assert.match(gateway,/api\('\/v1\/gateway\/hvn\/request'\)/);
assert.match(gateway,/api\('\/v1\/gateway\/hvn\/result'/);
assert.match(tauri,/"\/v1\/hvn"/);
assert.match(tauri,/fn hvn_research_request\(window_ms:u64\)/);
assert.match(api,/invoke\('hvn_research_request',\{windowMs\}\)/);
assert.match(workspace,/requestHvnResearch\(windowMs\)/);
assert.match(workspace,/account\?\.pending_orders===0/);
assert.match(ui,/<HvnWorkspace key=\{'hvn-'\+viewScope\}/);
assert.ok(ui.includes("page==='analysis'&&<HvnWorkspace"));
assert.ok(!file('crates/domain/src/execution_legacy.rs').includes('"hvn" =>'),'Research cannot become a silent live execution brain');
console.log('PASS: HVN Core, gateway, MT5 tick reader, Tauri, UI and execution isolation contracts');
