# BrainTrade OS — verified 0.7.0 Development Preview

Canonical repository: `gngoongoog/V4`. Current `development` runtime/source merge checkpoint: `4cf36107c72a59154859a8abdff64db70d6df21a` (tree `036989ffb18820920085fffa4b0517ace6e9904e`). PR #35 head `49ceb5956f9ac23aecaa03a444fa72abfc4fcf72` has the same tree and passed Research plus Windows/Linux CI before squash merge. This handoff is documentation/state synchronization only; it does not create new runtime evidence beyond those validated artifacts.

Current verified test scale is 82 domain Rust tests, 63 runtime Rust tests and 102 Python tests, with one explicit public-network official-news test intentionally ignored. Core lifecycle, backup/restore, Studio, chronological evaluation, correlation/OOS research, feature-ablation contracts, simulated MT5 execution, offline reports, browser review, native desktop rendering and Windows portable packaging passed.

## What is implemented

- 15 execution identities with coordinator/advisor/SOLO/RETURNING roles; Liquidity Sweep is the newest identity and earlier identity mappings remain unchanged.
- Scalping, Swing, Expert and Sniper modes.
- Durable broker intent/reconciliation, split-fill handling, account isolation, no blind resend after uncertain results and required final authorization before send.
- Portfolio Exposure v1, OFF by default, with aggregate slots/lots/estimated stop-risk gates that never block owned exits or reconciliation.
- Sniper Fast Lane plus execution-only `PUT /v1/execution/pulse`. The pulse reuses cached reconciled account/position/contract/frame state and updates only validated quote fields, avoiding learner/research work on every fast quote.
- Native Sniper MT5 bridge: `OnTick` wake + `CopyTicks(..., COPY_TICKS_ALL)` recovery, authenticated local named pipe, bounded FIFO, source-order quote delivery, same-millisecond event separation and automatic 5 ms Python quote-polling fallback.
- The MQL5 bridge is transport-only and contains no order submission API. Broker ownership stays in `gateway/execution.py`.
- Account-scoped commission profiles keyed by exact login/server/currency/symbol. Round-trip and Per-side input are supported; Per-side is normalized internally.
- Configurable Sniper maximum hold from 1–3600 seconds. Hold/target/stop are frozen into a trade at entry.
- Simplified native Control Center grouped into five primary sections: الرئيسية، التداول، البحث، المختبر، النظام. Trading mode, commission basis and Sniper hold are visible in the trading center.
- Historical Meta v1/v2, CES v1 and KTE v1 research; Wyckoff; Trend/Regime/Structure/Volatility/Gann/Liquidity analysis; deterministic replay/exports and chronological evaluation.
- Aligned multi-symbol correlation/exposure research with exact intersections, explicit unavailable data, persistence, replay and export. It is observational only and has no broker execution route.
- Holdout/rolling OOS evaluation infrastructure plus feature-group correlation/ablation tooling. Tooling is complete; genuine prospective evidence accumulation remains open.
- Learning snapshots/labels/export, offline challenger registry/evidence and read-only inference without automatic Champion promotion.
- Read-only Demo acceptance collector in the packaged `runtime\gateway\check_runtime.py`, with native/poll session separation and P50/P95/P99 characterization of stored real execution timings.

## Sniper latency boundary

The engineering target is **<=20 ms for BrainTrade-local queue-to-send latency** on the intended Windows machine. Characterize it from the stored fields, including `core_pulse_us`, `queue_to_gateway_ms`, `authorize_ms`, `journal_ms`, `pre_send_ms`, `queue_to_send_ms`, `send_ms` and `total_ms`.

Do not weaken the durable journal merely to improve a benchmark. `journal_ms` exists so its cost is measurable while preserving commit-before-`order_send` and crash/timeout no-blind-retry semantics. If the target is missed, first identify the dominant component. A fast queue with high `send_ms` points beyond the local decision path toward MT5/network/broker delay.

The <=20 ms figure is not a guarantee of MT5 acknowledgement, broker fill, network latency, profitability, every-tick capture or HFT status.

## CI evidence

Research run `39011639762`: SUCCESS.

- Artifact `12092852855` — `synthetic-research-validation`
- Size: `4301738` bytes
- SHA-256: `91b6e0b8273a60e90bd3c636fd55a66ffb7dee8b68e042399c8e623aa531da87`

Windows/Linux run `39011639745`: SUCCESS.

- Artifact `12093341260` — `BrainTrade-Windows-x64-Preview`
  - Size: `34402819` bytes
  - SHA-256: `6f85ae1562130da5da4c85bf0bff0d9cc07c88b6d4e84e89f3cb43137b3bd7da`
- Artifact `12093301318` — `windows-native-validation`
  - Size: `1701140` bytes
  - SHA-256: `6185b58db343063187aa97ae1d2f5e099e168d12e187652f00e39e064498a314`

`docs/development-validation.json` is the machine-readable development evidence record. Promoted 0.6 evidence in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` is immutable historical evidence and must not be replaced with 0.7 data.

## Target-machine work still pending

On Windows with the intended Demo account:

1. Follow `docs/QUICKSTART_AR.md`, `docs/ACCEPTANCE.md` and `docs/DEMO_ACCEPTANCE_AR.md`.
2. Compile/attach `BrainTradeSniperBridge.mq5`, verify the native path, then run a separate fallback-poll session.
3. Allow the strategy to trade normally; the collector itself must remain read-only and must never manufacture an order just to create a latency sample.
4. Collect useful real samples and preserve the JSON/CSV acceptance output, including P50/P95/P99 and broker result/retcode distributions.
5. Validate real fees, rejections, partial fills, account binding and native-to-fallback behavior.
6. If the local latency target is missed, optimize the measured bottleneck rather than removing identity, freshness, ownership, authorization or durable-journal protections.
7. Test private Telegram only with credentials stored locally.

## Scientific work still pending

- Accumulate genuine completed-bar observations, matured labels and independent holdout/rolling evidence.
- Use ablation results to test whether feature groups add unique prospective value before simplifying live logic.
- Add historical news/event replay with first-observed timestamps and causal joins.
- Keep correlation/exposure observational until separate prospective evidence and review justify any execution rule.
- Claim order-flow/volume-profile behavior only when the underlying feed supplies the required fields.

Startup/restart remains STOPPED. Native MT5 Demo acceptance and private Telegram delivery are not performed by CI. Imported ML automatic promotion is not implemented.
