"""P0-G UI broker-account generation guard over fully restored V24 Tauri UI.

Only modifies verified unique anchors in terminal.tsx and a Node contract test.
When a broker's currency/trade_mode changes without login or server changing,
account-scoped metrics are invalidated, and stale brain controls are rejected.
"""
from pathlib import Path

terminal=Path("apps/terminal/src/terminal.tsx")
s=terminal.read_text(encoding="utf-8")
old=""" const contextKey=`${status?.telemetry?.login??'unknown'}:${status?.telemetry?.server??'offline'}:${status?.telemetry?.symbol??'none'}`;"""
new=""" const contextKey=`${status?.telemetry?.login??'unknown'}:${status?.telemetry?.server??'offline'}:${status?.telemetry?.symbol??'none'}:${status?.telemetry?.currency??'unknown'}:${status?.telemetry?.account_trade_mode??'unknown'}:${status?.telemetry?.demo??'unknown'}`;"""
if s.count(old)!=1:raise SystemExit("P0-G unexpected Tauri context key; refuse release")
s=s.replace(old,new,1)
old="""  if(preview||!hasTauri()||!status?.gateway_fresh||!status.telemetry){sendNotice('يجب اتصال MT5 حديث لإدارة العقول',true);return}
  const t=status.telemetry;
  const context=JSON.stringify([t.server,t.login,t.symbol,t.currency]);
  const mode=String(unwrap(pageData).trading_mode||'');"""
new="""  if(preview||!hasTauri()||!status?.gateway_fresh||!status.telemetry||status.requested_run){sendNotice('أوقف التداول وانتظر اتصال MT5 حديث لإدارة العقول',true);return}
  const t=status.telemetry;
  if(t.account_trade_mode!==(t.demo?0:2)){sendNotice('نوع حساب MT5 غير مؤكّد؛ لا يمكن تغيير دور العقل',true);return}
  const context=JSON.stringify([t.server,t.login,t.symbol,t.currency]);
  const savedContext=String(unwrap(pageData).context||'');
  if(savedContext!==context){sendNotice('تغيّر الحساب؛ انتظر تحميل صفحة العقول للحساب الحالي',true);return}
  const mode=String(unwrap(pageData).trading_mode||'');"""
if s.count(old)!=1:raise SystemExit("P0-G unexpected advisor callback; refuse release")
s=s.replace(old,new,1)
terminal.write_text(s,encoding="utf-8")

checks=Path("apps/terminal/tests/contracts.cjs")
s=checks.read_text(encoding="utf-8")
append=r"""
// P0-G: stale MT5 Demo/REAL panels must be invalidated, and advisor action
// must match the connected broker + freshly queried page generation.
assert.match(terminal,/const contextKey=`[^`]*account_trade_mode/);
assert.match(terminal,/const contextKey=`[^`]*currency/);
assert.match(terminal,/if\(savedContext!==context\)/);
assert.match(terminal,/account_trade_mode!==\(t\.demo\?0:2\)/);
assert.match(terminal,/\|\|status\.requested_run/);
console.log('PASS: P0-G context scoped dashboard panels and protected brain controls');
"""
if 'P0-G: stale MT5 Demo/REAL panels' in s:raise SystemExit('P0-G tests unexpectedly already staged')
checks.write_text(s+append,encoding="utf-8")
print("P0-G restored Windows UI: broker mode/account cache isolation and protected advisor controls staged")
