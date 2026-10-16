# BrainTrade OS — verified 0.7.0 Development Preview

Canonical repository: `gngoongoog/V4`.

Current runtime/source merge checkpoint: `cc0ba8e84d862af706dbadec66c146c1fe47e2f2` (tree `488aa76c57c5b03719eeced3c2894ee2d065627f`). This merge includes deterministic Feature Ablation v1 on top of the already merged multi-symbol correlation A–D and persisted/replayable holdout-only OOS evidence.

The exact PR #28 source head `956dacd74fa8294a3cda8d759f161a3ab41ccb40` passed Research run `38959684403` and Windows/Linux run `38959684555` before merge. Verified exact-head scale was 78 domain Rust tests passed, 59 runtime Rust tests passed plus one intentionally ignored public-network news test, and 82 Python tests passed. Core lifecycle, backup/restore, Studio, simulated MT5 execution, offline reports, browser review, native desktop compilation/rendering and Windows portable packaging passed.

## What is implemented

- 14 execution identities with coordinator/advisor/SOLO/RETURNING roles.
- Scalping, Swing, Expert and Sniper modes.
- Durable broker intent/reconciliation, split-fill handling, account isolation and no blind resend after uncertain broker results.
- Portfolio Exposure v1, OFF by default, with aggregate slots/lots/estimated stop-risk entry gates that never block owned exits or reconciliation.
- Sniper Fast Lane with local timing fields and fast broker dispatch for tick entries/owned closes while preserving durable journal persistence before `order_send`.
- Native Sniper MT5 bridge: `OnTick` wake + `CopyTicks(..., COPY_TICKS_ALL)` recovery, authenticated local named pipe, bounded ordered delivery, same-millisecond event separation, partial-write recovery and automatic 5 ms Python quote-polling fallback.
- The MQL5 bridge is transport-only and contains no order submission API. Broker ownership stays in `gateway/execution.py`.
- Historical Meta v1/v2, CES v1 and KTE v1 research with deterministic replay/exports and chronological evaluation.
- Learning snapshots/labels/export, deterministic baseline trainer, challenger registry/evidence and read-only inference without automatic Champion promotion.
- Multi-symbol correlation research A–D with exact M1 availability masks, no forward filling, saved-backtest provenance, native UI, deterministic replay/export, restart/source-mutation checks and backup/restore coverage.
- Holdout-only OOS evidence with persistence/replay/restart/source-mutation protection.
- Feature Ablation v1 over the fixed 87-feature contract with leave-one-group-out retraining and cross-group redundancy diagnostics; diagnostic only, no automatic feature removal or trading effect.
- Native 19-page desktop and Windows portable package with embedded Python/MetaTrader5 runtime.

## Sniper latency boundary

The target is **<=20 ms BrainTrade-local queue-to-send latency** on an appropriate target Windows machine. Characterize it using `queue_to_gateway_ms`, `authorize_ms`, `journal_ms`, `pre_send_ms`, `queue_to_send_ms`, `send_ms` and `total_ms`, reporting P50/P95/P99. It is not a guarantee of MT5/broker acknowledgement, fill latency, network latency, profitability, HFT status or capture of every upstream broker event.

The native `CopyTicks` path reduces quote-observation loss caused by coalesced NewTick events by recovering terminal-cached ticks. It forwards Bid/Ask changes relevant to execution and leaves pure Last/volume-only ticks out of Sniper decisions. Target-machine acceptance remains mandatory.

## Research boundaries

Correlation, OOS and Ablation outputs are observational/research evidence only. They do not modify current Portfolio Exposure v1, coordinator votes, execution policy or automatic model promotion. Do not convert one inspected dataset/holdout into a profitability claim or automatic feature-selection rule.

The next research module is historical news/event replay. It must persist immutable observations and use `first_observed_ms` as the causality boundary. Provider `published_ms` is metadata, not proof that BrainTrade knew the event at publication time. Later corrections must not rewrite earlier knowledge.

## Evidence and delivery

`docs/development-validation.json` is the machine-readable development evidence record. Promoted 0.6 evidence in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` is immutable historical evidence and must not be replaced with 0.7 data.

Startup/restart remains STOPPED. Native MT5 Demo acceptance and private Telegram live delivery are not performed by CI. Imported ML automatic promotion is not implemented.

## Continue from here

1. Complete the docs/evidence synchronization using the post-merge workflows for `cc0ba8e...`.
2. Run native Windows + Demo MT5 acceptance from `docs/ACCEPTANCE.md`, including native bridge compilation/attachment, fallback, same-millisecond ordering, partial-write recovery, real fees/rejections/partial fills and latency distributions.
3. Test private Telegram using local credentials only.
4. Perform a focused Quality Cleanup of compiler/test/action warnings without changing trading semantics.
5. Add causal historical news/event replay with immutable observation history and deterministic replay/export/restart/backup tests.
6. Continue genuine OOS and Ablation evidence across multiple regimes/datasets.
7. Enable branch protection/required checks when administration access permits.
