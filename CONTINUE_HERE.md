# Continue BrainTrade 0.7.0 Development

GitHub `gngoongoog/V4` is canonical. Continue from `development`; do not restart from `main` unless preparing a promoted release.

## Current verified checkpoint

PR #50 (`Add one-click MT5 Demo acceptance operator`) is merged into `development` as `ae5cc5c5bf216fc8d2453b204dbc45abe2e89902`. Its exact tested head `3ff9425e2795102c8d84ea991260d985f2d4baba` passed Research run `39050497842` and Windows/Linux run `39050497836`. The tested PR head and squash merge share tree `db356f1eb5ae413543886f7a4f1427c883fad0da`.

The development line has 16 durable execution identities. Broker ownership remains append-only: `generated=410613`, `liquidity=410614`, `trend_pullback=410615`.

`docs/development-validation.json` is the machine-readable evidence record for this checkpoint. Promoted 0.6.0 evidence remains immutable and separate.

## Implemented engineering

- Coordinator/advisor/SOLO/RETURNING execution membership.
- Scalping, Swing, Expert and Sniper modes.
- Sniper Fast Lane, execution-only Fast Pulse and native MT5 `OnTick`/`CopyTicks` bridge with automatic polling fallback.
- Durable pre-send journal, uncertain-send reconciliation and no blind resend.
- Exact account binding, quote freshness, ownership and final authorization boundaries.
- Account-scoped commissions, configurable Sniper hold and Portfolio Exposure v1.
- Native five-section Control Center and persisted runtime state.
- Correlation/OOS/feature-ablation/news-replay research lanes that remain observational.
- Exact closed Demo broker-outcome adaptive learning and Intelligence V3 weighting.
- Trend Pullback using completed bars only with EMA20/EMA50 structure, controlled pullback, confirmation, anti-chasing and structural stop geometry.
- Packaged read-only Demo acceptance workflow: `Run-Demo-Acceptance.cmd`, `Tools\Run-Demo-Acceptance.ps1` and `DEMO_ACCEPTANCE_AR.md`.
- Windows parser validation of the acceptance PowerShell both before packaging and after staging the packaged copy.

## Continue in this order

1. **Run native Windows + real Demo MT5 acceptance on the target machine.** Open the packaged `Run-Demo-Acceptance.cmd`; collect separate `native` and `poll` sessions.
2. **Measure before further speed changes.** Use `queue_to_send_ms`, `core_pulse_ms`, `authorize_ms`, `journal_ms`, `other_pre_send_ms`, `pre_send_ms`, `send_ms` and `total_ms` to identify the actual bottleneck.
3. **Evaluate Trend Pullback with real Demo evidence.** Track closed outcomes, fees, spread/slippage, regime/session behavior and enough prospective/OOS samples before changing learned influence.
4. **Keep broker correctness boundaries intact.** Do not remove exact account identity, quote freshness, ownership, final authorization, durable journal or uncertain-send reconciliation merely to improve a benchmark.
5. **Keep imported ML challengers research-only** until independent evidence and an explicit promotion design exist.
6. **Test private Telegram delivery** only with target-machine credentials.
7. **Enable required branch checks** for `main` and preferably `development` if repository administration access is available.

## Important boundaries

- BrainTrade 0.7.0 is a Development Preview, not the promoted 0.6.0 release.
- Startup/restart stays STOPPED.
- The <=20 ms Sniper goal is BrainTrade-local queue-to-`order_send` handoff, not broker acknowledgement or fill latency.
- CI uses simulated broker boundaries and does not prove native MT5 acceptance, real fill quality, profitability or guaranteed latency.
- The Demo acceptance collector/operator is read-only and must not manufacture trades.
- Research correlation, ablation, news replay and imported ML do not automatically authorize broker execution.

When changing execution code, use a focused branch, preserve append-only broker magic ownership, run exact-head Research + Windows/Linux CI, then merge normally into `development`.
