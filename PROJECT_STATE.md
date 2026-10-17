# BrainTrade OS — project state: verified 0.7.0 Development Preview

Canonical repository: `gngoongoog/V4`. Current validated runtime/source merge checkpoint on `development`: `9524367f039b8fe59247e88c714c1cbff9c4a5a9` (tree `58a250ddca283e867436953a459959c33ee8a1e2`). PR #38 head `b6c15e8243cbf6792b88a9ca7bac3330643b619f` has the same runtime/source tree and passed Research plus Windows/Linux validation before squash merge.

Current verified test scale is 82 domain Rust tests, 68 runtime Rust tests and 109 Python tests; one explicit public-network official-news test is intentionally ignored. Core, backup/restore, Studio, chronological evaluation, OOS/correlation research, feature-ablation contracts, offline reports, simulated MT5 execution integration, browser review, native desktop rendering and Windows portable packaging passed. Native MT5 Demo acceptance on the user's actual terminal is still a separate target-machine gate.

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

The development branch contains deterministic historical Meta v1/v2, CES v1 and KTE v1 research; Wyckoff; Trend/Regime/Structure/Volatility/Gann/Liquidity analysis; official-news context; chronological evaluation; learning snapshot/label/export pipelines; read-only challenger inference; strategy generation across the supported MT5 timeframes; and causal official-news historical replay based on append-only `first_observed_ms` availability.

Aligned multi-symbol correlation/exposure research is now implemented and CI-verified. It uses exact common timestamp intersections and explicit unavailable data rather than forward-filling missing intervals. Reports are deterministic, persisted, replayable/exportable and explicitly observational; they cannot change broker execution or Portfolio Exposure v1.

Holdout/rolling out-of-sample infrastructure and feature-group correlation/ablation tooling are also implemented and verified. PR #37 added research-only historical official-news replay: publication time is metadata, while historical availability is controlled by BrainTrade's first-observed timestamp, with no forward fill across missing M1 bars and no execution route. This means the scientific machinery exists; it does **not** mean enough genuine prospective observations have accumulated to claim a profitable or stable edge.

## Real Demo learning state

PR #38 removed the remaining gap between simulated adaptive scoring and real Demo outcomes. When the bound MT5 account is Demo and a coordinator trade is closed, the runtime records exact broker evidence from the matched MT5 deals and calculates net outcome from broker-reported `profit + commission + swap + fee`. Only members that were ready and supported the executed side at entry receive that broker outcome. These broker samples are preferred immediately by the adaptive coordinator; the causal completed-bar learning path remains a fallback when no exact broker evidence exists.

This does not turn the project into unrestricted blind execution. The optional Portfolio Exposure and per-brain risk caps remain OFF by default, but exact account binding, quote freshness, ownership/reconciliation, duplicate prevention, final authorization and the durable commit-before-`order_send` journal remain mandatory correctness boundaries. Global START still has to be armed explicitly after startup/restart, which intentionally remains STOPPED.

## Demo Acceptance state

The Windows package now contains a read-only Demo acceptance collector in `runtime\gateway\check_runtime.py`. With `--demo-acceptance` it reads Core execution results and writes cumulative JSON/CSV reports without POST requests, START/STOP changes, account binding changes or broker orders.

It rejects Live/mismatched contexts, separates operator-labelled `native` and `poll` sessions, prevents mixing incompatible account/version state, hides the raw MT5 login behind a short fingerprint and calculates P50/P95/P99 for the stored execution timing fields. It can classify a completed sample as `TARGET_MET`, `CHARACTERIZED_SLOW` or `CONTRACT_FAIL`; until enough real sends exist it remains `INCOMPLETE`.

This collector makes target-machine acceptance measurable, but CI has not run it against the user's real MT5 Demo account. Therefore real fills/rejections/fees/partial fills and native-vs-poll latency distributions remain unverified.

## Evidence and delivery

`docs/development-validation.json` is the machine-readable 0.7 development evidence record. The promoted 0.6 history in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` remains unchanged. Documentation-only evidence commits must never be described as newly compiled runtime code.

Research run `39017294919` passed and uploaded `synthetic-research-validation` artifact `12095079199` (SHA-256 `412902f1ec2418f8d34c898a91a5823050894a4e9a41b1daeed2c3af3919c77f`). Windows/Linux run `39017294905` passed and uploaded `BrainTrade-Windows-x64-Preview` artifact `12095677572` (SHA-256 `10c4b0730691866129b3848940f96ab5ad4be1019d04e0d99cffd1d86ea2c8ba`) plus `windows-native-validation` artifact `12095269571` (SHA-256 `b764424d4d0cef36f03aba3000992a84b1ecfb62f5b539a8f121e9b61b9271fd`).

## Next steps

1. Run real Windows + Demo MT5 acceptance from `docs/DEMO_ACCEPTANCE_AR.md` and `docs/ACCEPTANCE.md`, collecting useful native and fallback samples.
2. If local P95/P99 misses the 20 ms queue-to-send target, optimize only the dominant measured component instead of removing durable safety boundaries blindly.
3. Continue genuine prospective/OOS accumulation and use ablation/correlation results as research evidence only.
4. Continue causal news/event replay research using the implemented first-observed infrastructure; keep it observational until independent evidence justifies any new hypothesis.
5. Test private Telegram delivery with credentials stored only on the target machine.
6. Enable required branch checks when repository administration access is available.

Startup/restart is STOPPED. SOLO never starts global execution automatically. STOP blocks new entries while owned timed exits continue with Core active; Full Exit stops client-side timed management while broker-attached SL/TP remain at the broker. Imported ML challengers remain research-only with no automatic Champion promotion. CI does not establish profitability, guaranteed latency or broker execution quality.
