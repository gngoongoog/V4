# BrainTrade OS — project state: verified 0.7.0 Development Preview

Canonical repository: `gngoongoog/V4`. Current `development` runtime/source merge checkpoint: `4cf36107c72a59154859a8abdff64db70d6df21a` (tree `036989ffb18820920085fffa4b0517ace6e9904e`). PR #35 head `49ceb5956f9ac23aecaa03a444fa72abfc4fcf72` has the same complete tree and passed Research plus Windows/Linux validation before squash merge.

Current verified test scale is 82 domain Rust tests, 63 runtime Rust tests and 102 Python tests; one explicit public-network official-news test is intentionally ignored. Core, backup/restore, Studio, chronological evaluation, OOS/correlation research, feature-ablation contracts, offline reports, simulated MT5 execution integration, browser review, native desktop rendering and Windows portable packaging passed. Native MT5 Demo acceptance on the user's actual terminal is still a separate target-machine gate.

## Execution and Sniper state

The package now includes 15 execution identities, including Liquidity Sweep; Scalping, Swing, Expert and Sniper modes; coordinator/advisor/SOLO/RETURNING membership; durable intent/reconciliation; split-fill and uncertain-send recovery; Portfolio Exposure v1; account-scoped commissions; configurable Sniper hold time; and the simplified native Control Center.

Sniper has two quote-arrival paths:

- a native Windows MT5 bridge (`BrainTradeSniperBridge.mq5`) that uses `OnTick` as the wake signal and `CopyTicks(..., COPY_TICKS_ALL)` to recover terminal-cached executable quote changes;
- the existing 5 ms Python `symbol_info_tick` loop as automatic fallback when the native bridge is absent or stale.

Fast Sniper quotes use an execution-only `PUT /v1/execution/pulse` path. A pulse carries only account identity, symbol, Bid/Ask and quote freshness fields. Core overlays only the quote onto cached reconciled state, rejects stale/wrong/crossed/duplicate input, runs execution/portfolio logic without rerunning the learner/research stack, and returns broker work in the same response. The final `/v1/execution/authorize` call remains mandatory.

The Python execution adapter still rechecks the account after authorization and retains the durable `broker-journal.sqlite` boundary with SQLite `synchronous=FULL`, commit-before-`order_send`, account/ownership checks and no blind retry after uncertain broker outcomes. Fast tick entries and owned closes use the fast dispatch path; regular orders still use broker `order_check`.

Current execution timing fields include `core_pulse_us`, `queue_to_gateway_ms`, `authorize_ms`, `journal_ms`, `pre_send_ms`, `queue_to_send_ms`, `send_ms` and `total_ms`. The target of **<=20 ms** refers only to BrainTrade-local queue-to-send latency on a suitable Windows target machine. It is not a broker acknowledgement/fill/network/profitability guarantee.

## Trading cost state

Commission configuration is account-scoped by exact login/server/currency/symbol identity. The operator can enter broker cost as either Round trip or Per side; Per-side values are normalized internally to round-trip cost. Global commission remains only as fallback when no exact account profile exists.

Sniper maximum hold is configurable from 1 to 3600 seconds and is frozen into the trade at entry together with its target/stop geometry. Changing the setting later does not mutate an already-open trade.

The native Control Center exposes the trading mode, commission basis, Sniper hold and current cost preview without requiring the operator to navigate the older button-heavy layout. Primary navigation is grouped into five sections: الرئيسية، التداول، البحث، المختبر، النظام.

## Research state

The development branch contains deterministic historical Meta v1/v2, CES v1 and KTE v1 research; Wyckoff; Trend/Regime/Structure/Volatility/Gann/Liquidity analysis; official-news context; chronological evaluation; learning snapshot/label/export pipelines; read-only challenger inference; and strategy generation across the supported MT5 timeframes.

Aligned multi-symbol correlation/exposure research is now implemented and CI-verified. It uses exact common timestamp intersections and explicit unavailable data rather than forward-filling missing intervals. Reports are deterministic, persisted, replayable/exportable and explicitly observational; they cannot change broker execution or Portfolio Exposure v1.

Holdout/rolling out-of-sample infrastructure and feature-group correlation/ablation tooling are also implemented and verified. This means the scientific machinery exists; it does **not** mean enough genuine prospective observations have accumulated to claim a profitable or stable edge.

## Demo Acceptance state

The Windows package now contains a read-only Demo acceptance collector in `runtime\gateway\check_runtime.py`. With `--demo-acceptance` it reads Core execution results and writes cumulative JSON/CSV reports without POST requests, START/STOP changes, account binding changes or broker orders.

It rejects Live/mismatched contexts, separates operator-labelled `native` and `poll` sessions, prevents mixing incompatible account/version state, hides the raw MT5 login behind a short fingerprint and calculates P50/P95/P99 for the stored execution timing fields. It can classify a completed sample as `TARGET_MET`, `CHARACTERIZED_SLOW` or `CONTRACT_FAIL`; until enough real sends exist it remains `INCOMPLETE`.

This collector makes target-machine acceptance measurable, but CI has not run it against the user's real MT5 Demo account. Therefore real fills/rejections/fees/partial fills and native-vs-poll latency distributions remain unverified.

## Evidence and delivery

`docs/development-validation.json` is the machine-readable 0.7 development evidence record. The promoted 0.6 history in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` remains unchanged. Documentation-only evidence commits must never be described as newly compiled runtime code.

Research run `39011639762` passed and uploaded `synthetic-research-validation` artifact `12092852855`. Windows/Linux run `39011639745` passed and uploaded `BrainTrade-Windows-x64-Preview` artifact `12093341260` plus `windows-native-validation` artifact `12093301318`.

## Next steps

1. Run real Windows + Demo MT5 acceptance from `docs/DEMO_ACCEPTANCE_AR.md` and `docs/ACCEPTANCE.md`, collecting useful native and fallback samples.
2. If local P95/P99 misses the 20 ms queue-to-send target, optimize only the dominant measured component instead of removing durable safety boundaries blindly.
3. Continue genuine prospective/OOS accumulation and use ablation/correlation results as research evidence only.
4. Add causal historical event/news replay with first-observed timestamps before testing event effects.
5. Test private Telegram delivery with credentials stored only on the target machine.
6. Enable required branch checks when repository administration access is available.

Startup/restart is STOPPED. SOLO never starts global execution automatically. STOP blocks new entries while owned timed exits continue with Core active; Full Exit stops client-side timed management while broker-attached SL/TP remain at the broker. Imported ML challengers remain research-only with no automatic Champion promotion. CI does not establish profitability, guaranteed latency or broker execution quality.
