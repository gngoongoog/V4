# BrainTrade OS — verified 0.7.0 Development Preview

Canonical repository: `gngoongoog/V4`. Current `development` runtime/source merge checkpoint: `7b127bf03fb45044ee53dc2366a6d0c53e7c59d8` (tree `10d1d6bee091b02fa1d044336649e645bce1b177`). PR #20 head `58e41284334d6778a3bfd2aa7eb5dd9cca5628fd` has the same tree and passed exact-head Research and Windows/Linux CI before merge. A later documentation-only evidence commit may record the completed post-merge checks; it does not change runtime provenance.

Current verified test scale is 61 domain Rust tests, 47 runtime Rust tests and 78 Python tests, with one explicit public-network news test intentionally ignored. Core lifecycle, backup/restore, Studio, simulated MT5 execution, offline report interactions, browser review, native selector compilation and 18-page native rendering passed in CI.

## What is implemented

- 14 execution identities with coordinator/advisor/SOLO/RETURNING roles.
- Scalping, Swing, Expert and Sniper modes.
- Durable broker intent/reconciliation, split-fill handling, account isolation and no blind resend after an uncertain broker result.
- Portfolio Exposure v1, OFF by default, with aggregate slots/lots/estimated stop-risk gates that never block exits or reconciliation.
- Sniper Fast Lane with measured internal timing fields and a fast broker dispatch path for tick entries/owned closes while preserving durable journal persistence before `order_send`.
- Native Sniper MT5 bridge: `OnTick` wake + `CopyTicks(..., COPY_TICKS_ALL)` recovery, authenticated local named pipe, bounded FIFO, source-order quote delivery, same-millisecond event separation and automatic 5 ms Python quote-polling fallback.
- The MQL5 bridge is transport-only and contains no order submission API. Broker ownership stays in `gateway/execution.py`.
- Historical Meta v1/v2, CES v1 and KTE v1 research, deterministic replay/exports and chronological evaluation.
- Learning snapshots/labels/export, offline challenger registry/evidence and read-only inference without automatic Champion promotion.
- Native 18-page desktop and Windows portable package with embedded Python/MetaTrader5 runtime.

## Sniper latency boundary

The target is **<=20 ms for BrainTrade-local queue-to-send latency** on an appropriate target Windows machine. This must be characterized from `queue_to_gateway_ms`, `authorize_ms`, `journal_ms`, `pre_send_ms`, `queue_to_send_ms`, `send_ms` and `total_ms`, using P50/P95/P99. It is not a guarantee of MT5/broker acknowledgement, fill latency, network latency, profitability or capture of every upstream broker event.

The native `CopyTicks` path reduces quote-observation loss caused by coalesced NewTick events by recovering terminal-cached ticks. It forwards Bid/Ask changes relevant to execution and leaves pure Last/volume-only ticks out of Sniper decisions. Target-machine acceptance remains mandatory.

## Evidence and delivery

`docs/development-validation.json` is the machine-readable development evidence record. Promoted 0.6 evidence in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` is immutable historical evidence and must not be replaced with 0.7 data.

Startup/restart remains STOPPED. Native MT5 Demo acceptance and private Telegram live delivery are not performed by CI. Imported ML automatic promotion is not implemented.

## Next repository phase

After evidence/state synchronization, begin aligned multi-symbol correlation/exposure **research only**. Align exact M1 timestamps across saved source runs, preserve explicit per-symbol availability masks, never forward-fill missing intervals, calculate pairwise statistics only on valid intersections and persist deterministic source identity. Do not connect correlation results to broker execution until separate prospective evidence and review exist.

## Target-machine work still pending

Follow `docs/QUICKSTART_AR.md` and `docs/ACCEPTANCE.md` on Windows with the intended Demo account. Compile/attach `MT5/Experts/BrainTradeSniperBridge.mq5`, verify native-to-fallback transitions, same-millisecond quote ordering, pipe interruption recovery, real fees/rejections/partial fills, and collect native/fallback latency distributions. Then test private Telegram using only local credentials.
