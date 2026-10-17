# Continue from verified BrainTrade 0.7.0 Development Preview

Current validated `development` runtime/source merge checkpoint is `9524367f039b8fe59247e88c714c1cbff9c4a5a9` (tree `58a250ddca283e867436953a459959c33ee8a1e2`). PR #38 head `b6c15e8243cbf6792b88a9ca7bac3330643b619f` has the same runtime/source tree and passed Research plus Windows/Linux validation before squash merge.

Read `BUILD_HANDOFF.md`, `PROJECT_STATE.md`, `ROADMAP.md`, `docs/development-validation.json`, `docs/ACCEPTANCE.md` and `docs/DEMO_ACCEPTANCE_AR.md` before changing runtime execution code. Do not restart the version bump, rewrite branch history or reuse old 0.6 artifact evidence as if it validated current development.

## Current implemented state

- 15 execution identities, including Liquidity Sweep.
- Coordinator/advisor/SOLO/RETURNING roles and Scalping/Swing/Expert/Sniper modes.
- Sniper Fast Lane plus execution-only Fast Pulse.
- Native MT5 OnTick/CopyTicks quote bridge with authenticated named pipe and automatic 5 ms Python fallback.
- Account-scoped commission profiles with Round-trip/Per-side basis.
- Configurable 1–3600 second Sniper hold frozen at entry.
- Portfolio Exposure v1, durable broker journal/reconciliation and no blind resend.
- Simplified five-section native Control Center.
- Correlation/exposure research, chronological OOS tooling and feature-group ablation tooling, all research-only.
- Read-only Demo acceptance collector for real Windows/MT5 latency characterization.
- Causal official-news historical replay using append-only first-observed timestamps; research-only and disconnected from execution.
- Real Demo broker-outcome learning: exact closed coordinator MT5 Demo deals feed adaptive member evidence using booked profit, commission, swap and fee.

The current CI checkpoint passed 82 domain Rust tests, 68 runtime Rust tests plus one intentionally ignored public-network test, and 109 Python tests. Research run `39017294919` and Windows/Linux run `39017294905` both passed.

## Continue in this order

1. **Run the real Demo acceptance session on the target Windows machine.** Use `docs/DEMO_ACCEPTANCE_AR.md`. Collect native OnTick samples and a separate 5 ms fallback-poll sample.
2. **Use measurements before further speed changes.** Compare P50/P95/P99 for `queue_to_send_ms` and inspect Core/pulse, authorization, journal, other pre-send and `send_ms` components.
3. **Do not remove durable execution boundaries just to hit a number.** Exact account identity, quote freshness, ownership, authorization, durable journal and uncertain-send reconciliation remain mandatory.
4. **Continue prospective scientific evidence.** Accumulate matured labels and holdout/rolling OOS evidence, then use ablation/correlation results to judge unique value.
5. **Use the implemented causal news replay only as research evidence** until independent prospective evidence justifies any execution hypothesis.
6. **Test private Telegram delivery** only with local credentials.
7. **Enable branch protection/required checks** if repository administration access becomes available.

## Important boundaries

- The <=20 ms goal is BrainTrade-local queue-to-`order_send` handoff, not fill latency.
- The Demo acceptance collector is read-only and must not manufacture trades.
- Correlation/exposure remains observational and cannot change broker execution.
- Imported ML challengers remain research-only and cannot automatically promote to Champion.
- Startup/restart stays STOPPED.
- CI uses simulated broker boundaries and does not prove real fill quality, profitability or guaranteed latency.

If target-machine acceptance uncovers a real failure or bottleneck, fix that measured issue on a focused feature branch, run exact-head Research + Windows/Linux CI, then merge normally into `development`.
