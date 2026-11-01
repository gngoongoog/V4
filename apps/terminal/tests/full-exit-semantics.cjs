// P0 full-exit guard contract: UI must never make an operator think that
// closing the window has stopped Core, and STOP may never close broker trades.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../../..');
const read=file=>fs.readFileSync(path.join(root,file),'utf8');
const ui=read('apps/terminal/src/terminal.tsx');
const bridge=read('apps/tauri/src/bridge.rs');
const core=read('crates/runtime/src/lib.rs');
const native=read('gateway/sniper_native.py');
assert.match(ui,/onCloseRequested\(event=>\{\s*event\.preventDefault\(\)/,'OS window X and Alt+F4 must open a choice');
assert.match(ui,/aria-label="خيارات إغلاق التطبيق"/,'custom X must no longer silently dismiss the UI');
assert.match(ui,/إغلاق الواجهة فقط — إبقاء Core/,'explicit presentation-only option must remain available');
assert.match(ui,/وقف الدخول والخروج الكامل/,'the true shutdown must have a separate confirmed action');
assert.match(ui,/await shutdownCore\(\);[\s\S]*?await closeTerminal\(\);/,'never close the UI before native shutdown is confirmed');
assert.match(bridge,/client\.checked_post\("\/v1\/stop",json!\(\{\}\),Duration::from_secs\(8\)\)/,'confirmed full exit must send STOP before requesting shutdown');
assert(bridge.indexOf('client.checked_post("/v1/stop"')<bridge.indexOf('client.shutdown(&instance)'),'STOP order before shutdown');
assert.match(bridge,/Err\(CheckedPostError::Uncertain\(reason\)\)/,'STOP uncertainty must keep startup fenced');
assert.match(bridge,/process\.wait\(\)\?;/,'must wait for the exact Core process exit');
assert.match(core,/verify_safe_shutdown\(&current,unresolved\)\?;/,'no bypass of broker-flat or pending intents');
assert.match(core,/t\.positions\.is_empty\(\) && t\.pending_orders == 0/,'never shutdown with known positions or orders');
assert.match(core,/stop_owned_gateway\(child,Duration::from_secs\(12\)\)/,'bound grace period for owned Gateway');
assert.match(core,/child\.kill\(\)/,'only owned Child may be terminated after the grace interval');
assert.match(core,/gateway_stop_mode/,'Core shutdown response must name the outcome');
assert.match(native,/def parse_packet\(raw: bytes \| bytearray \| str/,'bytearray parser regression must remain addressed');
console.log('PASS: full-exit auto-STOP, window-close choices, Core process wait, broker-flat gates and owned-Gateway cleanup');
