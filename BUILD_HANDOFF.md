# BrainTrade OS — Build Handoff

## Active task / user steering 2026-10-14

User requested that construction continue step by step with maximum precision. Build on `development`, keep `main` stable, and validate each coherent stage before the next. Windows packaging/native laptop acceptance remain deferred. No CI/backtest/ML metric is a profitability claim.

## Current verified development checkpoint

### Analytical / market layer

The live research layer contains the Market Opportunity Engine plus Trend, Regime, Structure, Volatility, Gann Geometry, broker-supported DOM Depth and Macro/News Context brains. Opportunity plans use completed-bar analysis plus current quote projection and expose structural stop/three targets/cost/lot gates. Stale inputs withdraw old Entry/SL/TP values. DOM is resting depth only when MT5 supplies Market Book; candle tick-volume/profile data are explicitly proxies, not exchange order flow. Macro/News uses official Fed/ECB RSS freshness and never converts headlines into BUY/SELL. New brains remain observational; live Shadow still uses legacy Meta and no broker execution exists.

Verified compile-only runs for the newer analytical stages:
- Market Opportunities: `38520989480` PASS.
- Gann Geometry: `38521328784` PASS.
- DOM Depth + gateway syntax: `38521812267` PASS.
- Macro/News Context: `38522309782` PASS.

### Learning Foundation v1

- `learning_snapshots` stores immutable closed-bar features once per account/server/symbol/currency + M1 bar.
- feature schema `gold-m1-feature-v1` uses reproducible closed-bar analytical features; quote projection, DOM and news are excluded until replay parity exists.
- `learning_labels` matures only after exact future M1 sequences for 5/15/30 minutes exist; gapped horizons are never shifted or fabricated.
- labels store ATR-normalized return/MFE/MAE and -1/0/+1 class at +/-0.20 ATR.
- `/v1/learning` and `/v1/learning/export` expose deterministic matured data.
- `scripts/train_linear_baseline.py` is offline-only: SHA-256 dataset identity, chronological per-context split, 30-row purge around split boundaries, train-only standardization, deterministic class-weighted 3-class softmax and untouched holdout metrics.
- model artifacts are always `unreviewed_challenger`, `eligible_for_promotion=false`.

### Model registry / challenger lifecycle

Runtime accepts validated research artifacts through `/v1/learning/register` and stores them immutably in `learning_models`. Registration validates model/feature/label schemas, target, lowercase SHA-256 dataset identity, minimum post-purge rows, purge metadata, chronological/manual-promotion contract, exact classes, finite dimensions/weights and train/validation/holdout metrics. Registration can assign only the `challenger` slot. There is no champion promotion, prediction or live self-modification.

A first full chronology/registry run exposed three Rust `rusqlite::query_map` lifetime errors only. Commit `48b1fb0fb6ae897f42f72a0a1452fb5df55e16da` fixed those iterator lifetimes without changing split, purge, causality or registry policy. Full research run `38524176106` then passed Rust tests, Core build, Python tests, Core smoke, browser review, native Desktop compile and artifact upload.

### Chronological Evaluation Engine v1

Domain evaluator `chronological-evaluation-v1` reuses the deterministic backtest engine and supports:
- explicit holdout and rolling development/holdout plans; no random split;
- Meta v1 vs Meta v2 on identical History/contract/cost/risk assumptions;
- at least two full days of real warmup before the first test window;
- fresh ledger and configured initial equity per window;
- entries strictly bounded to each window while warmup reads only earlier completed bars;
- UTC-day aligned definitions;
- tests for non-overlap, report bounds, rolling definitions, insufficient warmup/non-Meta rejection and future-mutation causality.

### Runtime Chronological Evidence Store — verified

Commit `e97723daa227255de295171d3e63642dad108f3e` adds persisted research evidence and Core routes. Full research run `38525014871` PASS: Rust domain/runtime tests, Core build, Python tests, Core smoke, offline browser review, native Desktop compile and artifact upload all succeeded.

- SQLite table `chronological_evaluations` stores immutable rows; there is no update/delete/promotion route.
- every evaluation binds to an exact saved `backtest_runs` source id plus immutable source identity: source creation time, algorithm/strategy/options, account/server/symbol/currency, history capture/first/last/bar count and contract.
- report/history identity and current algorithm identity are revalidated before evaluation.
- asynchronous worker supports cancellation/progress and never persists partial/panicked results.
- Core routes: `GET /v1/evaluation`, `POST /v1/evaluation/start`, `POST /v1/evaluation/cancel`, `GET /v1/evaluation/runs`, `POST /v1/evaluation/report`, `POST /v1/evaluation/export`.
- export writes normal files only: `evaluation.json`, `source_identity.json`, `manifest.json`.
- backup manifest schema 5 includes chronological evaluation evidence through the SQLite backup.
- every API/report explicitly keeps `prediction_enabled=false` and `promotion_enabled=false`.

## Next exact action

1. Add a dedicated Desktop chronological-evaluation review page using only the verified runtime API.
2. The UI must choose an existing saved Meta backtest as the source, choose Holdout or Rolling parameters, start/cancel, show progress, list immutable saved evaluations, display per-window/aggregate evidence and export it.
3. UI wording must state that windows are independent and results are not a connected equity curve or profitability proof.
4. Run the full research CI again, including native Desktop compilation, before moving on.
5. Only after the UI/evidence stage is green, implement manual challenger review-eligibility preconditions that reference separately persisted chronological evidence. Do not implement automatic promotion.
6. After registry + evidence + review policy are proven, add read-only challenger inference/logging only; it must not alter Shadow.

Keep live Shadow's source legacy Meta. No broker execution is enabled. `training_enabled=false`, `prediction_enabled=false`, `live_self_modification=false`. Never commit credentials, private runtime databases/logs, transport chunks, `.import`, encoded source, or ZIP-only source.
