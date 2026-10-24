"""Hardening regression contracts for two V22 broker entry/connection failures.

These static checks supplement (not substitute for) the Rust unit tests that
MUST run on a Windows build runner before distributing an EXE.
"""
from pathlib import Path
import unittest

ROOT=Path(__file__).resolve().parents[1]

class V23SafetyRegressions(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.execution=(ROOT/'crates/runtime/src/execution.rs').read_text('utf8')
        cls.supervisor=(ROOT/'crates/runtime/src/enterprise_supervisor.rs').read_text('utf8')
        cls.core=(ROOT/'crates/runtime/src/lib.rs').read_text('utf8')

    def test_corrupt_supervisor_policy_remains_hard_blocked_on_all_entry_paths(self):
        src=self.execution
        self.assertIn('supervisor_policy_valid: bool',src)
        self.assertIn('fn supervisor_permits_entry(&self, assessed: bool)->bool',src)
        self.assertIn('self.supervisor_policy_valid && (!self.supervisor.enforce_entry_gate || assessed)',src)
        self.assertIn('let entries_allowed=if run && self.supervisor_policy_valid',src)
        self.assertIn('let enter=if status.requested_run && self.supervisor_policy_valid',src)
        self.assertIn('corrupt_policy_cannot_open_trades_even_if_market_assessment_would_pass',src)
        self.assertNotIn('if run {\n            if !self.supervisor.enforce_entry_gate {true}',src)

    def test_corrupt_supervisor_rust_test_contains_valid_raw_json_string(self):
        src=self.supervisor
        self.assertIn('r#"{"enforce_entry_gate":"not-a-bool"}"#',src)
        self.assertNotIn('["{"enforce_entry_gate":"not-a-bool"}"]',src)
        self.assertIn('corrupt_persisted_policy_reports_blocked_instead_of_going_blank',src)

    def test_broker_reconfiguration_requires_fresh_reconciled_session(self):
        core=self.core
        self.assertIn('status.gateway_fresh && status.telemetry.as_ref().is_some_and(|t|t.connected)',core)
        self.assertIn('status.last_received_ms.is_some() || status.telemetry.is_some()',core)
        self.assertIn('Lost portfolio after reconnect must not authorize account switch',core)
        self.assertIn('Stale broker state must not authorize reconfiguration',core)
        self.assertIn('s.status.gateway_fresh=false;',core)
        reconnection=core[core.index('("POST","/v1/reconnect")'):core.index('("POST","/v1/gateway/snapshot")')]
        self.assertNotIn('s.status.last_received_ms=None',reconnection)
        self.assertIn('s.status.telemetry=None;',reconnection)

    def test_portable_release_requires_compiled_binary_and_smoke(self):
        script=(ROOT/'scripts/Build-V23-Portable-Windows.ps1').read_text('utf8')
        for evidence in ('Build-Tauri-Windows.ps1','smoke_portable_core.py',
                         'SHA256SUMS.json','verify_v23_release.py',
                         'EXE timestamp predates V23 build',
                         'package-lock.json','Cargo.lock'):
            self.assertIn(evidence,script)

if __name__=='__main__': unittest.main()
