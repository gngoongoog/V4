# Continue BrainTrade 0.7.0 Development

GitHub `gngoongoog/V4` is canonical. Continue from `development`; do not restart from `main` unless preparing a promoted release.

## Current verified checkpoint

PR #53 (`Add privacy-safe Demo acceptance export bundle`) is merged into `development` as `b20af1f0a3ae37d30e688e92615972e5c03e5adc`. Its exact tested head `31c10767e7c284160a74c1e0b001c0fde825780d` passed Research run `39058307544` and Windows/Linux run `39058307570`. The tested PR head and merge share tree `cb4cfc79358305d91caa4826ab2d47414fbd745b`.

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
- Packaged read-only Demo acceptance workflow with `Run-Demo-Acceptance.cmd`, a Windows GUI Acceptance Center, the advanced PowerShell CLI and Arabic instructions.
- GUI cards for result/sample counts/queue-to-send P95/P99/send P95/Native-vs-Poll timing plus controlled Native/Poll/Auto capture sessions.
- Privacy-safe acceptance evidence export using a strict allowlist; raw login, IPC key, broker databases/journals, Telegram credentials and GUI logs are excluded.
- Windows parser validation of the acceptance PowerShell source and packaged copies before delivery.

## Continue in this order

1. **Run native Windows + real Demo MT5 acceptance on the target machine.** Open the packaged `Run-Demo-Acceptance.cmd`; collect a `native` session with the bridge attached and a separate `poll` session with it detached/stopped.
2. **Export the share bundle.** Use `تصدير حزمة مشاركة` after the captures so the acceptance evidence can be reviewed without account secrets or broker databases.
3. **Measure before further speed changes.** Use `queue_to_send_ms`, `core_pulse_ms`, `authorize_ms`, `journal_ms`, `other_pre_send_ms`, `pre_send_ms`, `send_ms` and `total_ms` to identify the actual bottleneck.
4. **Evaluate Trend Pullback with real Demo evidence.** Track closed outcomes, fees, spread/slippage, regime/session behavior and enough prospective/OOS samples before changing learned influence.
5. **Keep broker correctness boundaries intact.** Do not remove exact account identity, quote freshness, ownership, final authorization, durable journal or uncertain-send reconciliation merely to improve a benchmark.
6. **Keep imported ML challengers research-only** until independent evidence and an explicit promotion design exist.
7. **Test private Telegram delivery** only with target-machine credentials.
8. **Enable required branch checks** for `main` and preferably `development` if repository administration access is available.

## Important boundaries

- BrainTrade 0.7.0 is a Development Preview, not the promoted 0.6.0 release.
- Startup/restart stays STOPPED.
- The <=20 ms Sniper goal is BrainTrade-local queue-to-`order_send` handoff, not broker acknowledgement or fill latency.
- CI uses simulated broker boundaries and does not prove native MT5 acceptance, real fill quality, profitability or guaranteed latency.
- The Demo acceptance collector/GUI is read-only and must not manufacture trades.
- The privacy-safe share exporter is diagnostic only and must stay allowlist-based; do not broaden it to runtime databases, secrets or raw account login data.
- Research correlation, ablation, news replay and imported ML do not automatically authorize broker execution.

When changing execution code, use a focused branch, preserve append-only broker magic ownership, run exact-head Research + Windows/Linux CI, then merge normally into `development`.
