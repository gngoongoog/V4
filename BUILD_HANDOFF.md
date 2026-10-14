# BrainTrade OS — Build Handoff

## Active steering — 2026-10-14

Continue construction step by step on `development`; keep `main` stable. Validate each coherent stage before starting the next. Windows packaging/native laptop acceptance remain deferred. No CI, backtest or ML metric is a profitability claim.

## Verified foundation

### Analytical / market layer

The live research layer contains Market Opportunities plus Trend, Regime, Structure, Volatility, Gann Geometry, broker-supported DOM Depth and Macro/News Context brains. Opportunity plans use completed-bar analysis plus current quote projection and expose structural stop/three targets/cost/lot gates. Stale inputs withdraw old Entry/SL/TP values. DOM is resting depth only when MT5 supplies Market Book; candle tick-volume/profile data are proxies, not exchange order flow. Macro/News uses official Fed/ECB RSS freshness and does not convert headlines into BUY/SELL. These newer brains remain observational; live Shadow still uses legacy Meta and no broker execution exists.

Verified compile-only runs: Market Opportunities `38520989480`, Gann `38521328784`, DOM `38521812267`, Macro/News `38522309782`.

### Learning Foundation v1

- `learning_snapshots`: immutable completed-bar features once per account/server/symbol/currency + M1 bar.
- feature schema `gold-m1-feature-v1`; quote projection, DOM and news are excluded until replay parity exists.
- `learning_labels`: exact future M1 sequences at 5/15/30m only; gapped horizons are never shifted or fabricated.
- labels include ATR-normalized return/MFE/MAE and -1/0/+1 class at +/-0.20 ATR.
- `/v1/learning` and `/v1/learning/export` expose deterministic matured data.
- `scripts/train_linear_baseline.py`: offline-only SHA-256 dataset identity, chronological per-context split, 30-row purge, train-only standardization, deterministic class-weighted 3-class softmax and untouched holdout metrics.
- registered artifacts remain `unreviewed_challenger`, `eligible_for_promotion=false`.

### Model registry / chronological evidence

`/v1/learning/register` stores validated artifacts immutably and can assign only the challenger slot. Chronological evaluation uses holdout/rolling development+holdout windows over saved immutable backtest history with causal warmup and independent ledgers. Runtime persists evidence in `chronological_evaluations` bound to exact backtest source identity.

Verified milestones:
- learning registry compile/lifetime fix: `48b1fb0fb6ae897f42f72a0a1452fb5df55e16da`, run `38524176106` PASS.
- chronological evidence store: `e97723daa227255de295171d3e63642dad108f3e`, run `38525014871` PASS.
- native chronological evaluation page: head `8ab1020c0ca5c4730441bfcd004e58f21be7bdb4`, run `38527014379` PASS.

### Manual Challenger Review Eligibility — VERIFIED

Normal source file: `crates/runtime/src/learning_review.rs`.
Policy: `manual-review-eligibility-v1`.

The gate verifies immutable model/feature/label/manual-promotion contracts, dataset identity, >=600 post-purge rows, untouched holdout metrics, >=30-row purge metadata and every distinct training context `(server, login, symbol, currency)`. Each context requires compatible separately persisted chronological evidence. Evidence/source identity and window boundaries are checked before an immutable `learning_review_records` row can be created.

Routes:
- `GET /v1/learning/review/status`
- `GET /v1/learning/review/records`
- `POST /v1/learning/review/create`
- `POST /v1/learning/review/report`

A review record is only a prerequisite record. It does not change model status, create a champion, enable prediction or alter Shadow.

Code head `9ab88714852c167b18c26b1277c8b15a15d54839`; full run `38528848123` PASS including Rust/Python/Core smoke/browser/native compile/headless render/artifact upload.

## Read-only Challenger Inference — VERIFIED

Normal source/test files:
- `crates/runtime/src/learning_inference.rs`
- `tests/learning_feature_parity.json`
- `tests/test_learning_features.py`
- `scripts/check_learning.py`

Important guarantees:
- inference is allowed only for the current `unreviewed_challenger` after a valid immutable review-eligibility record exists;
- it consumes the exact stored `learning_snapshots.features` payload used by dataset export instead of rebuilding a second feature pipeline;
- Rust reproduces the Python trainer's 87-feature contract, clipping, feature-name ordering, train means/stds, class order `[-1,0,1]` and stable softmax;
- a shared edge-case fixture is checked by both Python and Rust using canonical feature fingerprints;
- artifact feature-name drift fails closed;
- dataset identity must still match the registered immutable artifact;
- predictions are stored in `learning_predictions` with model id, dataset identity, review record, context, completed-bar time, target, class probabilities, predicted class and confidence;
- uniqueness is one prediction per `(model, context, bar)`;
- APIs are `GET /v1/learning/inference` and `GET /v1/learning/inference/recent`;
- `/v1/learning` includes review eligibility and read-only inference status;
- backup manifest schema 7 includes review records and prediction audit data.

The real-Core smoke now exercises the complete public API chain with synthetic data only:
Backtest -> immutable chronological evidence -> challenger registration -> review eligibility -> immutable review record -> read-only prediction -> backup -> restart persistence.
It also verifies no champion exists and the rule-based Shadow decision/execution flag are unaffected.

Verified final head for this stage: `7e1217f6462f4942949ea7efbc270ea68307c31c`.
Full research run `38531071859`: PASS for Rust domain/runtime tests, Core build, Python tests including cross-language feature parity, end-to-end Core smoke, browser report checks, native Desktop compile, all-page headless render and artifact upload.

## Current exact stage — Prediction Outcome Scoring

Build the audit layer that waits for an exact matured `learning_labels` row corresponding to each stored prediction, selects only the horizon encoded by that prediction's immutable target (5m/15m/30m), and then persists/aggregates out-of-sample prediction outcomes.

Required behavior:
1. Never fabricate or shift a missing future label. If the exact label is not matured, the prediction stays pending.
2. Bind the outcome to the immutable prediction/model/dataset/review record and the exact learning snapshot/label.
3. Persist realized class, correct/incorrect, probability assigned to the realized class, negative log loss and multiclass Brier score. Keep all values finite and validated.
4. Aggregate by model + target with sample count, class support/confusion, accuracy, balanced accuracy, macro-F1, mean log loss, mean Brier and confidence/calibration diagnostics.
5. Report small-sample status explicitly; do not convert metrics into promotion approval.
6. Expose read-only status/recent/metrics APIs and include the audit tables in backups.
7. Add deterministic unit tests plus a real-Core integration fixture where a prediction is initially pending, exact future bars mature its label, then one immutable scored outcome appears.
8. Model outputs still must not feed `Analysis.action`, Meta, opportunities, risk, lot sizing, Shadow or broker execution.
9. Champion promotion and live self-modification remain disabled.

After this stage is fully green, the next scientific step is a separate challenger performance-review policy over sufficiently large scored samples; it still must not auto-promote.

Never commit credentials, private runtime DB/logs, `.import`, chunks, encoded source, or ZIP-only source.
