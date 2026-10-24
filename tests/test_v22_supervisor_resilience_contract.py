import unittest
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]

class SupervisorRecoveryContract(unittest.TestCase):
    def test_rejected_policy_is_reported_not_hidden_or_enabled(self):
        text=(ROOT/'crates/runtime/src/enterprise_supervisor.rs').read_text('utf8')
        for check in ['safe.enforce_entry_gate=true', 'assessment.entry_allowed=false',
                      'assessment.recommended_mode=None', 'configuration_integrity',
                      'invalid_fail_closed', 'broker_execution_authorization',
                      'broker_flat_is_authoritative', 'broker_state_unknown_never_proves_flat',
                      'corrupt_persisted_policy_reports_blocked_instead_of_going_blank']:
            with self.subTest(check=check): self.assertIn(check,text)
    def test_ui_stops_trusting_disconnected_stream_and_resyncs(self):
        text=(ROOT/'apps/terminal/src/enterprise-supervisor.tsx').read_text('utf8')
        for check in ["setLoaded(false);setIntegrity('unknown');initialized.current=false",
                      'statusUnverified||blocked', '!loaded?',
                      "setEnforced(data.entry_gate_enforced===true)",
                      'disabled={preview||armed||busy||!loaded}']:
            with self.subTest(check=check): self.assertIn(check,text)
    def test_release_gates_and_safe_broker_disabled_windows_smoke_exist(self):
        text=(ROOT/'scripts/Build-V22-Portable-Windows.ps1').read_text('utf8')
        self.assertIn('smoke_portable_core.py',text)
        self.assertIn('Portable Core safety smoke failed: no release package will be produced',text)
        self.assertIn('verify_ui22_integrity.py',text)

if __name__=='__main__': unittest.main()
