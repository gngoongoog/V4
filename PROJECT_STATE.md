# BrainTrade OS — project state: verified 0.7.0 Development Preview

Canonical repository: `gngoongoog/V4`. Current runtime/source checkpoint on `development`: PR #48 squash merge `09c13460cee0a3fe42b4805ce1545529963783c7` (tree `7d952015f07c6022455500c12f321409cdc556c0`). PR #48 exact head `9c08109e6414132c0ddd07588bb55d3550d759be` has the same tree and passed Research plus Windows/Linux validation before merge.

Current exact-head CI scale is 87 domain Rust tests, 71 runtime Rust tests plus one intentionally ignored public-network news test, and 113 Python tests. Core, backup/restore, Studio, execution integration, offline reports, native desktop compilation/rendering, Linux validation and Windows portable packaging passed. Native MT5 Demo acceptance on the user's actual terminal remains a separate target-machine gate.

## Execution and Sniper state

The development package now includes 16 durable execution identities. Existing broker ownership is unchanged and append-only: `generated=410613`, `liquidity=410614`, and `trend_pullback=410615`. It also includes Scalping, Swing, Expert and Sniper modes; coordinator/advisor/SOLO/RETURNING membership; durable intent/reconciliation; split-fill and uncertain-send recovery; Portfolio Exposure v1; account-scoped commissions; configurable Sniper hold time; and the simplified native Control Center.

Sniper has two quote-arrival paths:

- a native Windows MT5 bridge (`BrainTradeSniperBridge.mq5`) using `OnTick` and `CopyTicks(..., COPY_TICKS_ALL)` to recover terminal-cached executable quote changes;
- the existing 5 ms Python `symbol_info_tick` loop as automatic fallback when the native bridge is absent or stale.

Fast Sniper quotes use the execution-only `PUT /v1/execution/pulse` path. Core overlays only the quote onto cached reconciled state, rejects stale/wrong/crossed/duplicate input, runs execution/portfolio logic without rerunning the learner/research stack, and returns broker work in the same response. Final `/v1/execution/authorize` remains mandatory.

The Python execution adapter rechecks the account after authorization and retains the durable `broker-journal.sqlite` boundary with SQLite `synchronous=FULL`, commit-before-`order_send`, account/ownership checks and no blind retry after uncertain outcomes. Fast tick entries and owned closes use the fast dispatch path; regular orders still use broker `order_check`.

Current timing fields include `core_pulse_us`, `queue_to_gateway_ms`, `authorize_ms`, `journal_ms`, `pre_send_ms`, `queue_to_send_ms`, `send_ms` and `total_ms`. The target of **<=20 ms** refers only to BrainTrade-local queue-to-send latency on a suitable Windows target machine. It is not a broker acknowledgement/fill/network/profitability guarantee.

## Trend Pullback state

PR #48 added the 16th execution identity, `trend_pullback`, without remapping any existing Magic ID. The assessor consumes completed bars only and waits for an established EMA20/EMA50 trend, a controlled pullback toward EMA20 that preserves the EMA50 structure, then a confirmation candle back with the trend.

It blocks late entries when the confirmation close is too far from EMA20 or the current executable quote has already moved too far from the confirmation. Its stop is structural rather than a fixed arbitrary price: the recent pullback/confirmation extreme is buffered by ATR, with sanity bounds before the signal can become executable. Reward distance remains the configured per-brain reward/risk multiple. CI proves causal/source contracts and execution wiring; it does not prove the strategy is profitable.

## Trading cost state

Commission configuration is account-scoped by exact login/server/currency/symbol identity. The operator can enter broker cost as either Round trip or Per side; Per-side values are normalized internally to round-trip cost. Global commission remains only as fallback when no exact account profile exists.

Sniper maximum hold is configurable from 1 to 3600 seconds and is frozen into the trade at entry together with target/stop geometry. Changing the setting later does not mutate an already-open trade.

The native Control Center exposes trading mode, commission basis, Sniper hold and current cost preview. Primary navigation is grouped into five sections: الرئيسية، التداول، البحث، المختبر، النظام.

## Research and adaptive-learning state

The development branch contains deterministic historical Meta v1/v2, CES v1 and KTE v1 research; Wyckoff; Trend/Regime/Structure/Volatility/Gann/Liquidity analysis; official-news context; chronological evaluation; learning snapshot/label/export pipelines; read-only challenger inference; strategy generation; causal official-news replay; aligned multi-symbol correlation/exposure research; holdout/rolling OOS infrastructure; and feature-group correlation/ablation tooling.

Correlation, ablation, news replay and imported ML remain research-only. They do not directly authorize broker execution. Imported ML challengers have no automatic Champion promotion path.

Exact closed Demo broker outcomes feed the adaptive coordinator when a bound MT5 Demo trade is reconciled. Net evidence comes from broker-reported `profit + commission + swap + fee`, and only coordinator members that were ready and supported the executed side at entry receive that outcome. This is real-outcome learning logic, but CI still uses a simulated MT5 boundary.

## Demo Acceptance state

The Windows package contains the read-only Demo acceptance collector in `runtime\gateway\check_runtime.py`. With `--demo-acceptance` it reads Core execution results and writes cumulative JSON/CSV reports without POST requests, START/STOP changes, binding changes or manufactured broker orders.

It rejects Live/mismatched contexts, separates operator-labelled `native` and `poll` sessions, prevents mixing incompatible account/version state, fingerprints the login instead of storing it raw, and calculates P50/P95/P99 for stored execution timing fields. It can classify a completed sample as `TARGET_MET`, `CHARACTERIZED_SLOW` or `CONTRACT_FAIL`; until enough real sends exist it remains `INCOMPLETE`.

CI has not run this collector against the user's actual Windows + MT5 Demo installation. Real fills/rejections/fees/partial fills and native-vs-poll latency distributions therefore remain unverified.

## Evidence and delivery

`docs/development-validation.json` records the exact PR #48 checkpoint. The promoted 0.6 history in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` remains unchanged.

Research run `39045218522` passed on exact head `9c08109e6414132c0ddd07588bb55d3550d759be` and uploaded `synthetic-research-validation` artifact `12105916152` (SHA-256 `7e2ad28069847ced6d9ea01720bd5bdf8f21af178c7b5cc7afec3137652f7352`). Windows/Linux run `39045218508` passed on the same head and uploaded `BrainTrade-Windows-x64-Preview` artifact `12105678547` (SHA-256 `9ec11add63d8908ae82628aac00138503c949410dbb6b03504cef7f13203c5a8`) plus `windows-native-validation` artifact `12105628870` (SHA-256 `c9adc928ea48b5c465560a6a0d29ebc433adc85d85f92546b7412b495efc15cd`).

## Next steps

1. Run real Windows + Demo MT5 acceptance from `docs/DEMO_ACCEPTANCE_AR.md` and `docs/ACCEPTANCE.md`, collecting useful native and fallback samples.
2. If local P95/P99 misses the 20 ms queue-to-send target, optimize only the dominant measured component instead of removing durable correctness boundaries.
3. Accumulate genuine Trend Pullback and coordinator Demo/OOS evidence, including spread, fees, slippage and regime/session behavior.
4. Continue causal research lanes as observational evidence until independent results justify any promotion design.
5. Test private Telegram delivery with credentials stored only on the target machine.
6. Enable required branch checks when repository administration access is available.

Startup/restart is STOPPED. SOLO never starts global execution automatically. STOP blocks new entries while owned timed exits continue with Core active; Full Exit stops client-side timed management while broker-attached SL/TP remain at the broker. CI does not establish profitability, guaranteed latency or broker execution quality.