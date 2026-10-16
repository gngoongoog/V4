# BrainTrade OS — Roadmap

GitHub `gngoongoog/V4` is canonical. Read `BUILD_HANDOFF.md`, `PROJECT_STATE.md`, `CONTINUE_HERE.md`, `docs/development-validation.json` and `docs/ACCEPTANCE.md` before changing runtime code.

## Promoted 0.6.0 checkpoint

Verified executable/source checkpoint: `fdc63d4d05df89e27607641bd197b9e6ead936e9` (tree `8593783f43a4892cdc724ed5d112607ef7cee006`). Research CI `38803539711`: PASS. Windows CI/package `38803539405`: PASS. PR #13 promoted the reviewed 0.6.0 line to `main` without rewriting history.

Promoted 0.6 evidence in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` is historical and must not be replaced by 0.7 development evidence.

## Current 0.7.0 Development Preview

Current runtime/source merge checkpoint: `cc0ba8e84d862af706dbadec66c146c1fe47e2f2` (tree `488aa76c57c5b03719eeced3c2894ee2d065627f`). This includes PR #28 deterministic learning feature ablation on top of the previously merged correlation and OOS research work.

The exact PR #28 source head `956dacd74fa8294a3cda8d759f161a3ab41ccb40` passed Research run `38959684403` and Windows/Linux run `38959684555` before merge. The current test scale observed on that source is 78 domain Rust tests passing, 59 runtime Rust tests passing plus one intentionally ignored public-network news test, and 82 Python tests passing. Post-merge validation of `cc0ba8e...` is tracked separately in `docs/development-validation.json` once complete.

Startup/restart remains STOPPED. CI exercises simulated MT5 boundaries; native Windows + Demo MT5 acceptance on the intended terminal remains a separate target-machine gate.

## Completed engineering foundation

- 14 execution identities with coordinator/advisor/SOLO/RETURNING roles.
- Scalping, Swing, Expert and Sniper modes.
- Durable broker intent journal/reconciliation, uncertain-result no-blind-retry, account isolation and split-fill handling.
- Portfolio Exposure v1, OFF by default, with aggregate position/lot/estimated-stop-risk entry gates that never block owned exits or reconciliation.
- Sniper Fast Lane with local component timings and fast tick-entry/owned-close dispatch while retaining durable journal persistence before `order_send`.
- Native MT5 Sniper quote bridge: `OnTick` wake, `CopyTicks(..., COPY_TICKS_ALL)` terminal-cache recovery, authenticated local pipe, source-order delivery, same-millisecond event separation, partial-write recovery and automatic 5 ms Python polling fallback.
- The MQL5 bridge is transport-only; it contains no broker order submission API.
- Native 19-page desktop and portable Windows package with embedded Python/MetaTrader5 runtime.

The Sniper engineering target is **<=20 ms BrainTrade-local queue-to-send latency** on an appropriate target machine. P50/P95/P99 must be measured on the user's Windows + Demo MT5 setup. This is not a guarantee of MT5/broker acknowledgement, fill latency, network latency, capture of every upstream event, profitability or HFT performance.

## Completed research foundation

- Historical Meta v1/v2, CES v1 and KTE v1 with immutable inputs, causal replay, experiments and ledgers.
- Trend, Regime, Structure, Volatility, Gann, broker DOM Depth and Macro/News context.
- Chronological evaluation and explicit no-lookahead guards.
- Learning snapshots/labels/export, deterministic baseline trainer, challenger registry, read-only inference and manual review/performance evidence.
- **Multi-symbol correlation research A–D**:
  - exact M1 alignment and explicit per-symbol availability masks;
  - no forward-fill through missing intervals;
  - pairwise statistics only on valid intersections with explicit minimum overlap;
  - common all-symbol intersection for portfolio covariance/volatility;
  - deterministic source identity;
  - persistence/API over immutable saved backtests only;
  - native correlation UI in the 19-page dashboard;
  - deterministic replay/export, restart persistence, source-mutation detection and backup/restore coverage.
- **OOS evidence**:
  - holdout-only aggregation separated from Development windows;
  - deterministic identity and fail-closed non-finite/missing evidence handling;
  - persistence, replay, restart survival and source-mutation detection under the evaluation research boundary.
- **Feature Ablation v1**:
  - deterministic leave-one-feature-group-out retraining over the existing 87-feature contract;
  - same purged chronological split and baseline trainer as the Learning Foundation;
  - validation/holdout deltas versus the full model;
  - cross-group high-correlation diagnostics for possible duplicated information;
  - diagnostic only: no automatic feature selection, model promotion, prediction activation or broker effect.

## Immediate engineering work

1. Complete native Windows + Demo MT5 acceptance using `docs/ACCEPTANCE.md`, including native/fallback Sniper P50/P95/P99 local latency characterization, real broker rejections/fees/partial fills and restart/reconciliation cases.
2. Test private Telegram delivery only with credentials stored on the target machine.
3. Enable branch protection / required checks for `main` and preferably `development` when repository administration access is available.
4. Run a focused quality cleanup: remove verified dead/unused Rust warnings, close the test subprocess pipe that emits a `ResourceWarning`, and update deprecated GitHub Actions where compatible. Keep this separate from trading behavior.

## Next research phase — historical news/event replay

Build this as research-only evidence first; do not connect it to execution, risk or coordinator voting.

- Persist immutable news/event observations instead of only the latest feed cache.
- Record `first_observed_ms` separately from provider `published_ms`. A headline published at 10:00 but first observed by BrainTrade at 10:15 must be unavailable to any replay before 10:15.
- Preserve source identity and sanitized payload revision history; later feed corrections must not rewrite what was known earlier.
- Join events to historical bars/evaluation windows only by information actually observed at or before the replay timestamp.
- Add deterministic replay/export, restart and backup/restore coverage.
- Add synthetic tests for late observation, provider timestamp corrections, duplicate URLs/events, revised payloads, missing publication time and source failure.
- Keep headlines observational in v1: no fabricated BUY/SELL sentiment and no broker action.
- Do not claim retroactive `first_observed_ms` for periods before BrainTrade began storing observation history.

## Scientific work still open

- Accumulate genuine completed-bar observations, matured labels and repeated holdout/OOS evidence across meaningful regimes.
- Use Ablation v1 diagnostically across multiple datasets/regimes; do not tune to one inspected holdout.
- Add historical news/event replay with first-observed causal timestamps.
- Consider correlation-driven portfolio policy only after separate prospective evidence and review; current Portfolio Exposure v1 does not consume correlation research.
- True feed-supported order flow / volume profile only when the source actually provides the required fields.

## Explicitly not complete

- Native broker acceptance on the user's actual MT5 installation.
- Live Telegram acceptance.
- Imported ML Champion activation or automatic promotion.
- Automatic feature removal from Ablation results.
- Correlation-driven broker execution policy.
- News-driven broker execution policy.
- Guaranteed capture of every upstream broker event.
- Any profitability, HFT-latency or execution-quality guarantee.

A module is complete only when code, exact-head verification and state documentation agree.
