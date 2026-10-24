"""Preflight source guard for BRAINTRADE V23. Does not replace cargo tests."""
from pathlib import Path
import re

root=Path(__file__).resolve().parents[1]
supervisor=(root/'crates/runtime/src/enterprise_supervisor.rs').read_text('utf-8')
execution=(root/'crates/runtime/src/execution.rs').read_text('utf-8')
core=(root/'crates/runtime/src/lib.rs').read_text('utf-8')
checks={
 'Rust malformed JSON literal repaired': 'r#"{"enforce_entry_gate":"not-a-bool"}"#' in supervisor and '["{"enforce_entry_gate":"not-a-bool"}"]' not in supervisor,
 'corrupt policy retains independent hard block': 'supervisor_policy_valid: bool' in execution and 'self.supervisor_policy_valid && (!self.supervisor.enforce_entry_gate || assessed)' in execution,
 'normal Core execution obeys policy integrity': 'let entries_allowed=if run && self.supervisor_policy_valid' in execution,
 'Fast Pulse obeys policy integrity': 'let enter=if status.requested_run && self.supervisor_policy_valid' in execution,
 'broker account config requires reconciled fresh state': 'status.gateway_fresh && status.telemetry.as_ref().is_some_and(|t|t.connected)' in core,
 'reconnect retains session marker': 's.status.gateway_fresh=false;' in core[core.index('("POST","/v1/reconnect")'):core.index('("POST","/v1/gateway/snapshot")')],
 'Rust regression tests for two failures': 'corrupt_policy_cannot_open_trades_even_if_market_assessment_would_pass' in execution and 'Lost portfolio after reconnect must not authorize account switch' in core,
 'SAFE startup + shutdown smoke is part of release': 'BRAINTRADE_GATEWAY_DISABLED' in (root/'scripts/smoke_portable_core.py').read_text('utf-8'),
}
for label,passed in checks.items():print(('PASS' if passed else 'FAIL')+' '+label)
assert all(checks.values()),'V23 source release integrity failed'
print('PASS V23 static preflight; native Rust build and Demo acceptance must still be run')
