# BrainTrade OS — verified 0.7.0 Development Preview

Canonical repository: `gngoongoog/V4`. Current validated runtime/source merge checkpoint: `9524367f039b8fe59247e88c714c1cbff9c4a5a9` (tree `58a250ddca283e867436953a459959c33ee8a1e2`). PR #38 head `b6c15e8243cbf6792b88a9ca7bac3330643b619f` has the same runtime/source tree and passed Research plus Windows/Linux CI before squash merge. Later documentation-only synchronization must not be described as new runtime evidence.

Current verified test scale is 82 domain Rust tests, 68 runtime Rust tests and 109 Python tests, with one explicit public-network official-news test intentionally ignored. Core lifecycle, backup/restore, Studio, chronological evaluation, correlation/OOS research, feature-ablation contracts, simulated MT5 execution, offline reports, browser review, native desktop rendering and Windows portable packaging passed.

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
- Causal official-news historical replay from PR #37. `first_observed_ms`, not publication time alone, controls historical availability; missing M1 bars are never forward-filled and the replay has no execution route.
- Real Demo broker-outcome learning from PR #38. Closed coordinator Demo trades become exact broker evidence using booked profit, commission, swap and fee; only members that supported the executed side receive the result. Broker evidence is preferred over the completed-bar fallback.
- Aligned multi-symbol correlation/exposure research with exact intersections, explicit unavailable data, persistence, replay and export. It is observational only and has no broker execution route.
- Holdout/rolling OOS evaluation infrastructure plus feature-group correlation/ablation tooling. Tooling is complete; genuine prospective evidence accumulation remains open.
- Learning snapshots/labels/export, offline challenger registry/evidence and read-only inference without automatic Champion promotion.
- Read-only Demo acceptance collector in the packaged `runtime\gateway\check_runtime.py`, with native/poll session separation and P50/P95/P99 characterization of stored real execution timings.

## Sniper latency boundary

The engineering target is **<=20 ms for BrainTrade-local queue-to-send latency** on the intended Windows machine. Characterize it from the stored fields, including `core_pulse_us`, `queue_to_gateway_ms`, `authorize_ms`, `journal_ms`, `pre_send_ms`, `queue_to_send_ms`, `send_ms` and `total_ms`.

Do not weaken the durable journal merely to improve a benchmark. `journal_ms` exists so its cost is measurable while preserving commit-before-`order_send` and crash/timeout no-blind-retry semantics. If the target is missed, first identify the dominant component. A fast queue with high `send_ms` points beyond the local decision path toward MT5/network/broker delay.

The <=20 ms figure is not a guarantee of MT5 acknowledgement, broker fill, network latency, profitability, every-tick capture or HFT status.

## CI evidence

Research run `39017294919`: SUCCESS.

- Artifact `12095079199` — `synthetic-research-validation`
- Size: `4303555` bytes
- SHA-256: `412902f1ec2418f8d34c898a91a5823050894a4e9a41b1daeed2c3af3919c77f`

Windows/Linux run `39017294905`: SUCCESS.

- Artifact `12095677572` — `BrainTrade-Windows-x64-Preview`
  - Size: `34472951` bytes
  - SHA-256: `10c4b0730691866129b3848940f96ab5ad4be1019d04e0d99cffd1d86ea2c8ba`
- Artifact `12095269571` — `windows-native-validation`
  - Size: `1701140` bytes
  - SHA-256: `b764424d4d0cef36f03aba3000992a84b1ecfb62f5b539a8f121e9b61b9271fd`

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
- Continue using the implemented first-observed historical news replay for observational causal research; do not connect it to execution without separate evidence and review.
- Keep correlation/exposure observational until separate prospective evidence and review justify any execution rule.
- Claim order-flow/volume-profile behavior only when the underlying feed supplies the required fields.

Startup/restart remains STOPPED. Native MT5 Demo acceptance and private Telegram delivery are not performed by CI. Imported ML automatic promotion is not implemented.
