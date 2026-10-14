# BrainTrade OS — Build Handoff

## Active task / user steering 2026-10-14

Continue construction step by step with maximum precision on `development`; keep `main` stable. Validate each coherent stage before the next. Windows packaging/native laptop acceptance remain deferred. No CI/backtest/ML metric is a profitability claim.

## Verified foundation

### Analytical / market layer

The live research layer contains Market Opportunities plus Trend, Regime, Structure, Volatility, Gann Geometry, broker-supported DOM Depth and Macro/News Context brains. Opportunity plans use completed-bar analysis plus current quote projection and expose structural stop/three targets/cost/lot gates. Stale inputs withdraw old Entry/SL/TP values. DOM is resting depth only when MT5 supplies Market Book; candle tick-volume/profile data are proxies, not exchange order flow. Macro/News uses official Fed/ECB RSS freshness and does not convert headlines into BUY/SELL. These newer brains remain observational; live Shadow still uses legacy Meta and no broker execution exists.

Verified compile-only runs: Market Opportunities `38520989480`, Gann `38521328784`, DOM `38521812267`, Macro/News `38522309782`.

### Learning Foundation v1

- `learning_snapshots`: immutable closed-bar features once per account/server/symbol/currency + M1 bar.
- feature schema `gold-m1-feature-v1`; quote projection, DOM and news are excluded until replay parity exists.
- `learning_labels`: exact future M1 sequences at 5/15/30m only; gapped horizons are never shifted/fabricated.
- labels include ATR-normalized return/MFE/MAE and -1/0/+1 class at +/-0.20 ATR.
- `/v1/learning` and `/v1/learning/export` expose deterministic matured data.
- `scripts/train_linear_baseline.py`: offline-only SHA-256 dataset identity, chronological per-context split, 30-row purge, train-only standardization, deterministic class-weighted 3-class softmax and untouched holdout metrics.
- artifacts remain `unreviewed_challenger`, `eligible_for_promotion=false`.

### Model registry / challenger lifecycle

`/v1/learning/register` stores validated artifacts immutably in `learning_models` and may assign only the `challenger` slot. It validates schemas/target/dataset SHA/post-purge rows/purge metadata/chronological contract/classes/finite model vectors and split metrics. No champion promotion, inference or live self-modification exists at this checkpoint.

Commit `48b1fb0fb6ae897f42f72a0a1452fb5df55e16da` fixed only three `rusqlite::query_map` lifetime errors. Full research run `38524176106` then passed Rust tests, Core build, Python tests, Core smoke, browser review and native Desktop compile.

### Chronological Evaluation + Evidence Store

`chronological-evaluation-v1` reuses the deterministic backtest engine with explicit holdout/rolling development+holdout windows, real pre-window warmup, fresh ledger/equity per window, UTC-day boundaries and future-mutation causality tests. Runtime persists immutable evidence in `chronological_evaluations` bound to exact saved backtest source identity and exposes start/cancel/list/report/export routes. Commit `e97723daa227255de295171d3e63642dad108f3e`, full run `38525014871` PASS.

The dedicated native Desktop chronological-evaluation review page was subsequently implemented and headless-render validated. Verified head before the current review-policy stage: `8ab1020c0ca5c4730441bfcd004e58f21be7bdb4`; full run `38527014379` PASS.

## Current stage — Manual Challenger Review Eligibility

Code head under validation: `9ab88714852c167b18c26b1277c8b15a15d54839`.
Workflow run: `38528848123`.

New normal source file: `crates/runtime/src/learning_review.rs`.

Policy `manual-review-eligibility-v1` is intentionally a prerequisite gate only:
- verifies the current challenger still obeys immutable model/feature/label/manual-promotion contracts;
- verifies dataset identity, >=600 post-purge rows, untouched holdout metrics and >=30-row purge metadata;
- parses every distinct training context `(server, login, symbol, currency)`;
- requires valid separately persisted chronological evidence for every training context, not merely one context;
- validates evidence/source identity, development+holdout presence and that Meta v1/v2 reports remain bounded to their declared windows;
- creates immutable `learning_review_records` only when the submitted evidence set exactly covers the policy-selected evidence;
- never changes model status, never creates a champion, never enables model prediction, never alters Shadow.

Core routes:
- `GET /v1/learning/review/status`
- `GET /v1/learning/review/records`
- `POST /v1/learning/review/create`
- `POST /v1/learning/review/report`

`GET /v1/learning` is enriched with computed manual-review eligibility. Backup manifest schema is now 6 and records review-eligibility data while explicitly keeping model inference/promotion disabled.

Tests added:
- every training context needs separate compatible chronological evidence;
- malformed/out-of-window evidence is rejected;
- persistence does not create a champion or change the challenger status.

### CI status at this checkpoint

For run `38528848123` on `9ab8871...`, the following are already PASS:
- Rust domain/runtime tests;
- Core build;
- Python unit tests;
- Core smoke;
- browser/offline report interaction check;
- native Desktop compilation on Linux.

At checkpoint write time, the final headless Desktop page render and artifact upload were still running/pending. Do not call the stage fully green until that run concludes SUCCESS.

## Next exact action

1. Confirm run `38528848123` completes fully SUCCESS. Fix the same stage first if it does not.
2. Once green, build **read-only challenger inference/logging** only.
3. Rust inference must reproduce the offline trainer feature contract exactly by feature name, normalization means/stds, class ordering and softmax; add parity tests before trusting output.
4. Inference may run only for the current immutable challenger after a persisted valid manual-review eligibility record exists.
5. Store model id, dataset identity, review record, account context, closed-bar time, target, class probabilities and predicted class in a separate immutable audit table with one prediction per model/context/bar.
6. Read-only predictions must never feed `Analysis.action`, risk, lot sizing, opportunities, Meta, Shadow or broker execution.
7. Only after enough separately labeled read-only predictions exist should a later stage evaluate challenger calibration/out-of-sample performance. No automatic promotion.

Keep live Shadow's source legacy Meta. No broker execution is enabled. No live self-modification exists. Never commit credentials, private runtime DB/logs, `.import`, chunks, encoded source, or ZIP-only source.
