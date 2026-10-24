import unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
class V21SupervisorContract(unittest.TestCase):
 def test_closed_candle_and_ohlc_guards_are_present(self):
  s=(ROOT/'crates/runtime/src/enterprise_supervisor.rs').read_text(encoding='utf-8')
  for x in ['frame_max_age_seconds','trustworthy_candles','non-monotonic or duplicate candle timestamps','invalid OHLC candle evidence']:
   with self.subTest(x=x):self.assertIn(x,s)
 def test_advisory_and_enforcement_not_confused(self):
  ui=(ROOT/'apps/terminal/src/enterprise-supervisor.tsx').read_text(encoding='utf-8')
  self.assertIn('setEnforced(data.entry_gate_enforced===true)',ui)
  self.assertIn('const blocked=enforced && evidenceProblem;',ui)
 def test_no_pulse_claimed_fresh_only_from_reported_age(self):
  s=(ROOT/'crates/runtime/src/execution_pulse.rs').read_text(encoding='utf-8')
  self.assertIn('now.saturating_sub(pulse.quote_time_ms) <= 1500',s)
 def test_rust_regressions_cover_stale_and_corrupt_bars(self):
  s=(ROOT/'crates/runtime/src/enterprise_supervisor.rs').read_text(encoding='utf-8')
  self.assertIn('fn fresh_snapshot_with_old_candles_must_not_authorize_new_entries()',s)
  self.assertIn('fn out_of_order_duplicate_or_malformed_bar_never_passes()',s)
if __name__=='__main__':unittest.main()
