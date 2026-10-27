"""Read-only V21 source invariants; Rust compilation and MT5 acceptance remain separate."""
from pathlib import Path
root=Path(__file__).resolve().parents[1]
supervisor=(root/'crates/runtime/src/enterprise_supervisor.rs').read_text(encoding='utf-8')
pulse=(root/'crates/runtime/src/execution_pulse.rs').read_text(encoding='utf-8')
core=(root/'crates/runtime/src/execution.rs').read_text(encoding='utf-8')
ui=(root/'apps/terminal/src/enterprise-supervisor.tsx').read_text(encoding='utf-8')
checks={
  'closed candle recency rule': 'fn trustworthy_candles(' in supervisor and 'now_s.saturating_sub(last) > frame_max_age_seconds(tf)' in supervisor,
  'OHLC sanity checks': 'invalid OHLC candle evidence' in supervisor,
  'ordered timestamps': 'non-monotonic or duplicate candle timestamps' in supervisor,
  'Swing uses hourly ATR': 'TradingMode::Swing {("H1",h1)}' in supervisor,
  'all modes have independent frames': 'TradingMode::Swing => &[("H1",h1),("H4",h4),("D1",d1)]' in supervisor,
  'advisory freshness': 'trustworthy_candles(m5,"M5",min,now).ok()' in supervisor,
  'pulse age verified from exchange time': 'now.saturating_sub(pulse.quote_time_ms) <= 1500' in pulse,
  'fast pulse path still subject to guard': 'let enter=if status.requested_run' in core and 'self.supervisor.enforce_entry_gate' in core,
  'dashboard displays effective persisted policy': 'setEnforced(data.entry_gate_enforced===true)' in ui and 'تحذير غير مُفَعَّل' in ui,
  'Rust regression tests': all(x in supervisor for x in [
      'fresh_snapshot_with_old_candles_must_not_authorize_new_entries',
      'out_of_order_duplicate_or_malformed_bar_never_passes',
      'stale_m5_or_m15_never_produces_an_expert_recommendation',
      'swing_uses_h1_atr_and_requires_higher_frame_time_evidence',
      ]),
}
for description,ok in checks.items():print(('PASS' if ok else 'FAIL'), description)
assert all(checks.values()),'Source contract is incomplete'
print('V21 STATIC CONTRACT: PASS — not a compiled Rust binary certification')
