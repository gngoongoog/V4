# Continue BrainTrade 0.7.0 Development

GitHub `gngoongoog/V4` is canonical. Continue from `development`; do not restart from `main` unless preparing a promoted release.

## Current verified checkpoint

PR #48 (`Add causal Trend Pullback execution brain`) is merged into `development` as `09c13460cee0a3fe42b4805ce1545529963783c7`. Its exact PR head `9c08109e6414132c0ddd07588bb55d3550d759be` passed Research run `39045218522` and Windows/Linux run `39045218508`. The tested PR head and the squash merge share tree `7d952015f07c6022455500c12f321409cdc556c0`, so the merged runtime/source tree is the tree that was validated.

The development line now has 16 durable execution identities. Broker ownership remains append-only: `generated=410613`, `liquidity=410614`, and the new `trend_pullback=410615`.

`docs/development-validation.json` is the machine-readable evidence record for this checkpoint. The promoted 0.6.0 evidence remains immutable and separate.

## Implemented engineering

- Coordinator/advisor/SOLO/RETURNING execution membership.
- Scalping, Swing, Expert and Sniper modes.
- Sniper Fast Lane, execution-only Fast Pulse, and native MT5 `OnTick`/`CopyTicks` bridge with authenticated local transport and automatic fallback polling.
- Durable pre-send journal, uncertain-send reconciliation and no blind resend.
- Exact account/server/currency binding, quote freshness and owned-position execution boundaries.
- Account-scoped commissions, configurable Sniper hold and Portfolio Exposure v1.
- Native five-section Control Center and persisted runtime state.
- Correlation/OOS/feature-ablation/news-replay research lanes that remain observational.
- Exact closed Demo broker-outcome adaptive learning for coordinator participants.
- Intelligence V3 weighting with reward/confidence, regime/session context and execution-quality evidence.
- Trend Pullback execution identity using completed bars only: EMA20/EMA50 trend structure, controlled pullback, confirmation quality, anti-chasing distance and structural stop geometry.

## Continue in this order

1. **Run native Windows + real Demo MT5 acceptance on the target machine.** Follow `docs/DEMO_ACCEPTANCE_AR.md` and `docs/ACCEPTANCE.md`; collect separate native and fallback-poll samples.
2. **Measure before further speed changes.** Use `queue_to_send_ms`, `core_pulse_ms`, `authorize_ms`, `journal_ms`, `other_pre_send_ms`, `pre_send_ms`, `send_ms` and `total_ms` to identify the actual bottleneck.
3. **Evaluate Trend Pullback with real Demo evidence.** Track closed outcomes, fees, spread/slippage, regime/session behavior and enough prospective/OOS samples before changing its learned influence.
4. **Keep broker correctness boundaries intact.** Do not remove exact account identity, quote freshness, ownership, final authorization, durable journal or uncertain-send reconciliation merely to improve a benchmark.
5. **Keep imported ML challengers research-only** until independent evidence and an explicit promotion design exist.
6. **Test private Telegram delivery** only with target-machine credentials.

## Important boundaries

- BrainTrade 0.7.0 is a Development Preview, not the promoted 0.6.0 release.
- Startup/restart stays STOPPED.
- The <=20 ms Sniper goal is BrainTrade-local queue-to-`order_send` handoff, not broker acknowledgement or fill latency.
- CI uses simulated broker boundaries and does not prove native MT5 acceptance, real fill quality, profitability or guaranteed latency.
- The Demo acceptance collector is read-only and must not manufacture trades.
- Research correlation, ablation, news replay and imported ML do not automatically authorize broker execution.

When changing execution code, use a focused branch, preserve append-only broker magic ownership, run exact-head Research + Windows/Linux CI, then merge normally into `development`.