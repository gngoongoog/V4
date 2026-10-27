"""Static V22 release invariants; DOES NOT certify Windows compilation or MT5 trading."""
from pathlib import Path
root = Path(__file__).resolve().parents[1]
read = lambda path: (root / path).read_text(encoding='utf-8')
core = read('crates/runtime/src/enterprise_supervisor.rs')
ui = read('apps/terminal/src/enterprise-supervisor.tsx')
smoke = read('scripts/smoke_portable_core.py')
checks = {
 'invalid settings fail closed': 'safe.enforce_entry_gate=true' in core,
 'unknown broker portfolio never assumed flat': 'broker_flat_is_authoritative' in core and 'self.execution.account_matches(&status)' in read('crates/runtime/src/lib.rs'),
 'policy corruption read-only diagnosis': '"configuration_integrity"' in core and '"invalid_fail_closed"' in core,
 'entry gate does not represent broker authorization': 'broker_execution_authorization' in core,
 'persisted policy unchanged by display': 'setEnforced(data.entry_gate_enforced===true)' in ui,
 'loss of Core connection loses trusted UI status': "setLoaded(false);setIntegrity('unknown')" in ui,
 'portable smoke cannot place trades': "BRAINTRADE_GATEWAY_DISABLED='1'" in smoke and '/v1/shutdown' in smoke and '/v1/status' in smoke,
 'portable smoke requires bundled Gateway': "'runtime' / 'gateway' / 'mt5_gateway.py'" in smoke,
 'native pulse freshness checks retained': 'now.saturating_sub(pulse.quote_time_ms) <= 1500' in read('crates/runtime/src/execution_pulse.rs'),
}
for desc, yes in checks.items():
    print(('PASS' if yes else 'FAIL'), desc)
assert all(checks.values()), 'V22 invariant failure'
print('V22 source invariants PASS (native build and Demo acceptance remain untested)')
