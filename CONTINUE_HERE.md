# Continue from verified BrainTrade 0.7.0 Development Preview

Current runtime/source merge checkpoint: `cc0ba8e84d862af706dbadec66c146c1fe47e2f2` (tree `488aa76c57c5b03719eeced3c2894ee2d065627f`). The exact PR #28 source head `956dacd74fa8294a3cda8d759f161a3ab41ccb40` passed Research `38959684403` plus Windows/Linux `38959684555` before merge. Read `BUILD_HANDOFF.md`, `PROJECT_STATE.md`, `ROADMAP.md`, `docs/development-validation.json` and `docs/ACCEPTANCE.md` before changing runtime code. Do not restart the version bump or reuse promoted 0.6 evidence as 0.7 evidence.

Implemented development work now includes Portfolio Exposure v1, four modes, SOLO/RETURNING roles, adaptive coordinator evidence, durable execution recovery, Sniper Fast Lane, native MT5 Sniper quote transport, multi-symbol correlation research A–D, holdout-only OOS persistence/replay and deterministic Feature Ablation v1.

The native Sniper path uses `OnTick` to wake, `CopyTicks(..., COPY_TICKS_ALL)` to recover terminal-cached ticks, an authenticated local named pipe and distinct same-millisecond event ordering. The 5 ms Python quote poll remains automatic fallback. The EA is transport-only; broker orders remain exclusively in the journaled Python execution adapter.

The Sniper engineering target is <=20 ms for BrainTrade-local queue-to-send latency, measured from stored component timings. Do not turn that target into a broker-fill, network, profitability or HFT guarantee. Native Windows/Demo measurements are still required.

Correlation, OOS and Ablation are research-only. Correlation does not alter Portfolio Exposure v1 or execution. OOS evidence does not promote a model. Ablation does not automatically remove features or activate predictions.

## Continue in this order

1. Finish this development state/evidence synchronization using post-merge Research and Windows results for `cc0ba8e...`; keep promoted 0.6 records unchanged.
2. Run native Windows + Demo MT5 acceptance from `docs/ACCEPTANCE.md`, including native bridge attachment, fallback, same-millisecond ordering, partial-write recovery, broker fees/rejections/partial fills/restart and P50/P95/P99 local latency characterization.
3. Test private Telegram delivery only with local credentials.
4. Run **Quality Cleanup** as its own PR: remove verified dead/unused warnings without changing trading logic, close the test subprocess pipe that raises a `ResourceWarning`, and update deprecated GitHub Actions where compatible.
5. Add **historical news/event replay — research only**:
   - immutable observation history;
   - `first_observed_ms` as the causality boundary, separate from provider `published_ms`;
   - no retroactive knowledge before BrainTrade actually observed the event;
   - deterministic replay/export, restart and backup coverage;
   - no fabricated BUY/SELL sentiment and no execution/risk/coordinator effect.
6. Continue genuine prospective OOS/Ablation evidence across multiple regimes and datasets; do not tune to one inspected holdout.
7. Enable branch protection / required checks for `main` and preferably `development` when repository administration access is available.

Startup/restart stays STOPPED. Imported ML challengers remain research-only and cannot automatically promote to Champion. Correlation-driven and news-driven broker execution are not implemented. CI validates software behavior with simulated MT5 boundaries; it does not prove real broker execution quality or profitability.
