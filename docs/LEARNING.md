# BrainTrade Learning Foundation

This document describes the implemented research-only learning pipeline on `development`. It is not an autonomous trading AI and does not authorize broker execution.

## Safety boundary

- Training is offline; the live Core does not retrain itself.
- Registered learned models do not control Shadow.
- Champion promotion/activation is disabled and has no API route.
- Learned artifacts cannot submit, modify or close MT5 orders.
- Legacy Meta remains the live Shadow decision source.
- Metrics are research evidence, not a profitability or win-rate guarantee.

## 1. Immutable completed-bar features

Core stores one `learning_snapshots` row per account/server/symbol/currency context and completed M1 bar using schema `gold-m1-feature-v1`.

Inputs reproducible from saved completed candles include Trend, Regime, Structure, Volatility, Meta-v2 candidate state, multi-timeframe momentum/divergence, normalized Gann geometry, confirmed zones/setups, candle tick-activity/profile proxies and UTC session geometry. Current bid/ask projection, live DOM and live News/Macro context remain excluded from v1 learned features until replay/timestamp parity exists.

## 2. Exact delayed labels

Schema: `gold-m1-outcome-v1`.

Labels mature only when exact future M1 bars exist for 5, 15 and 30 minutes. Each horizon records ATR-normalized return, MFE, MAE and class `-1/0/1` around the ±0.20 ATR threshold. Missing, duplicated, out-of-order or gapped horizons are not shifted or fabricated.

## 3. Deterministic dataset export

`POST /v1/learning/export` writes matured feature+label rows in deterministic database order. Dataset identity is SHA-256 bound and later model/review/inference layers verify that identity.

## 4. Offline baseline trainer

`scripts/train_linear_baseline.py` implements a deterministic class-weighted 3-class softmax baseline with chronological context splits, 30-row purge around boundaries, train-only standardization, no random split, no parameter search and no automatic promotion. Artifacts report train/validation/untouched-holdout classification metrics and preserve split/context metadata.

## 5. Immutable challenger registry

`POST /v1/learning/register` validates the model/feature/label schema contract, dataset identity, minimum post-purge rows, chronological split metadata, class order, dimensions and finite values. A valid artifact is stored immutably and may occupy only the challenger slot with `eligible_for_promotion=false`.

## 6. Persisted chronological evidence

The chronological evaluation service runs Meta v1/v2 over frozen saved backtest History using explicit holdout/rolling windows, causal warmup and independent ledgers. Results are persisted with exact source identity in `chronological_evaluations`. They are evidence records, not promotion decisions.

## 7. Manual review eligibility

Policy: `manual-review-eligibility-v1`.

A review record requires a valid immutable challenger, matching dataset identity, at least 600 post-purge rows, untouched holdout metrics, >=30-row purge metadata and compatible chronological evidence for every distinct training context. The resulting record is only a prerequisite/audit record; it does not create a champion or enable trading influence.

Routes:
- `GET /v1/learning/review/status`
- `GET /v1/learning/review/records`
- `POST /v1/learning/review/create`
- `POST /v1/learning/review/report`

## 8. Read-only challenger inference

After a valid review-eligibility record exists, the current challenger may produce audit predictions from the exact stored `learning_snapshots.features` payload. Runtime inference reproduces the trainer's 87-feature ordering, clipping, means/stds, class order `[-1,0,1]` and stable softmax. Cross-language fixtures guard parity and feature drift fails closed.

Predictions persist model ID, dataset identity, review record, context, completed-bar time, target, probabilities, predicted class and confidence. They never change `Analysis.action`, Meta, opportunities, risk, lot sizing or Shadow.

## 9. Prediction outcome scoring

Policy/version: `prediction-outcome-scoring-v1`.

Scoring waits for the exact matured learning label corresponding to the prediction and selects only the immutable target horizon (5m/15m/30m). It persists realized class, correctness, true-class probability, log loss, multiclass Brier score, calibration error and realized return/MFE/MAE.

Aggregate read-only metrics by model+target include sample/pending counts, class/predicted support, confusion matrix, accuracy, balanced accuracy, macro-F1, mean log loss, mean Brier, mean confidence and 10-bin ECE. Small samples are explicitly warned; the current reporting floors are not promotion thresholds. Every result remains `promotion_eligible=false`, `shadow_effect=false`, `broker_effect=false`.

Routes:
- `GET /v1/learning/scoring`
- `GET /v1/learning/scoring/recent`

## 10. Next layer

The next scientific layer is a separate manual challenger performance-review policy over sufficiently large persisted scored outcomes. It must bind exact immutable evidence, reject insufficient/context-mismatched samples and produce at most a human-review recommendation. Champion activation and any Shadow/broker influence remain later, separate design/validation work.

## Prospective inference boundary (0.4.0)

Registered model split metadata must include `holdout_last_bar`. Inference only
accepts known model contexts and feature bars strictly later than the maximum
holdout boundary across **all** contexts plus 30 minutes (the maximum label horizon).
This prevents in-sample observations or labels used to build/evaluate the artifact
from being scored as prospective evidence. Older artifacts lacking the boundary
must be retrained/re-exported; they are not silently granted inference eligibility.

## Native performance review

The Learning page exposes current performance details, freezes immutable reviews and opens saved reports by ID from the review list. POST /v1/learning/performance-review/create requires model_id, evidence_signature and optional note (500 characters). A changed subject or evidence set is rejected. Fingerprint v2 includes regime slices and exact serialized metrics; fixed v1 policy thresholds remain unchanged. No Champion or trading effect is enabled.
