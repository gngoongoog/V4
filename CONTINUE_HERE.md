# Continue BrainTrade 0.7.0 Development

GitHub `gngoongoog/V4` is canonical. Continue from `development`; do not restart from `main` unless preparing a promoted release.

## Current development line

`development` already includes the 0.7 execution program through Intelligence V3. PR #47 merged Intelligence V3 at merge commit `30cc06390603491b7396f8c0c60a61c3d8416306` after its exact PR head passed Research and Windows/Linux CI.

PR #48 is the next focused continuation: it appends a causal `trend_pullback` execution identity without remapping any previous broker ownership. If PR #48 is merged, the durable identity ceiling is 16 brains and `trend_pullback` owns magic `410615`; `generated=410613` and `liquidity=410614` remain unchanged. Always verify the PR state before assuming this candidate is present on `development`.

The formal `docs/development-validation.json` record may intentionally lag later merged engineering until a new exact-head validation record is prepared. Never rewrite old validation evidence to make it appear newer than the CI run that produced it.

## Implemented engineering

- Coordinator/advisor/SOLO/RETURNING execution membership.
- Scalping, Swing, Expert and Sniper modes.
- Sniper Fast Lane, execution-only Fast Pulse, and native MT5 `OnTick`/`CopyTicks` bridge with authenticated local transport and automatic fallback polling.
- Durable pre-send journal, uncertain-send reconciliation and no blind resend.
- Exact account/server/currency binding, quote freshness and owned-position execution boundaries.
- Account-scoped commissions, configurable Sniper hold and Portfolio Exposure v1.
- Native Control Center and persisted runtime state.
- Correlation/OOS/feature-ablation/news-replay research lanes that remain observational.
- Exact closed Demo broker-outcome adaptive learning for coordinator participants.
- Intelligence V3 weighting with reward/confidence, regime/session context and execution-quality evidence.
- PR #48 candidate: Trend Pullback based on completed bars, EMA20/EMA50 trend structure, controlled pullback, confirmation quality, anti-chasing distance and structural stop geometry.

## Continue in this order

1. **Finish exact-head CI for any active feature PR before merge.** Research and Windows/Linux workflows must both pass on the current PR head.
2. **Run native Windows + real Demo MT5 acceptance on the target machine.** Follow `docs/DEMO_ACCEPTANCE_AR.md` and `docs/ACCEPTANCE.md`; collect separate native and fallback-poll samples.
3. **Measure before further speed changes.** Use `queue_to_send_ms`, `core_pulse_ms`, `authorize_ms`, `journal_ms`, `other_pre_send_ms`, `pre_send_ms`, `send_ms` and `total_ms` to identify the actual bottleneck.
4. **Keep broker correctness boundaries intact.** Do not remove exact account identity, quote freshness, ownership, final authorization, durable journal or uncertain-send reconciliation merely to improve a benchmark.
5. **Accumulate genuine prospective/OOS evidence.** New execution brains are hypotheses until measured; CI correctness is not profitability evidence.
6. **Keep imported ML challengers research-only** until independent evidence and an explicit promotion design exist.
7. **Test private Telegram delivery** only with target-machine credentials.

## Important boundaries

- BrainTrade 0.7.0 is a Development Preview, not the promoted 0.6.0 release.
- Startup/restart stays STOPPED.
- The <=20 ms Sniper goal is BrainTrade-local queue-to-`order_send` handoff, not broker acknowledgement or fill latency.
- CI uses simulated broker boundaries and does not prove native MT5 acceptance, real fill quality, profitability or guaranteed latency.
- The Demo acceptance collector is read-only and must not manufacture trades.
- Research correlation, ablation, news replay and imported ML do not automatically authorize broker execution.

When changing execution code, use a focused branch, preserve append-only broker magic ownership, run exact-head Research + Windows/Linux CI, then merge normally into `development`.