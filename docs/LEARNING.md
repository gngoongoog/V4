# BrainTrade Learning Foundation

This document describes the current learning pipeline on `development`. It is a research data pipeline and model registry, not an autonomous trading AI.

## Safety boundary

Current invariants:

- `training_enabled = false` inside the live Core.
- `prediction_enabled = false` inside Shadow.
- `champion_promotion_enabled = false`.
- `live_self_modification = false`.
- No learned artifact can submit, modify or close an MT5 order.
- Legacy Meta remains the live Shadow decision source.

A high historical score is not a profitability guarantee and is not enough to promote a model.

## 1. Closed-bar feature capture

Core stores one immutable `learning_snapshots` row per account/server/symbol/currency context and completed M1 bar.

Feature schema: `gold-m1-feature-v1`.

Included inputs are reproducible from completed candle history: Trend, Regime, Structure, Volatility, Meta-v2 candidate state, M1/M5/M15 momentum/divergence, Gann-inspired normalized geometry, confirmed zone geometry, setup metadata, MT5 candle tick-activity/profile proxies and UTC session geometry.

Deliberately excluded from v1 learning features are current bid/ask entry projection, live DOM / Market Book, News/Macro context and every future-derived value. Those feeds stay excluded until historical replay parity and timestamp alignment exist.

## 2. Delayed outcome labels

Label schema: `gold-m1-outcome-v1`.

A label is not written when the feature is captured. Core waits until exact future M1 bars exist for 5, 15 and 30 minutes. Each horizon stores final ATR-normalized return, MFE, MAE and direction class `-1/0/1` using ±0.20 ATR around zero.

If a required minute is missing, duplicated, out of order or gapped, the label is not shifted to another bar and is not fabricated.

## 3. Dataset export

`POST /v1/learning/export` writes matured feature+label rows only to `exports/learning-dataset-<timestamp>/dataset.jsonl` with deterministic database ordering. `GET /v1/learning` reports snapshot/label counts, registry slots, recent registered models and all disabled execution flags.

## 4. Offline baseline trainer

Run only after enough matured data exists:

```text
python scripts/train_linear_baseline.py <learning-export-folder>
```

The trainer has no network, MT5 or Core dependency. It uses a deterministic 3-class softmax linear baseline, fixed transparent features, train-only standardization, train-only class weights, no random split, no parameter search and no automatic promotion.

Each account context is split chronologically about 60% / 20% / 20%. Because labels inspect up to 30 future minutes, 30 rows are purged on each side of split boundaries. Training is refused if post-purge data is too small or any partition lacks one of the three classes.

The artifact records train/validation/holdout accuracy, balanced accuracy, macro F1, log loss, mean confidence, confusion matrix and majority baseline. These are dataset metrics, not expected profit or broker execution quality.

## 5. Immutable challenger registration

Model schema: `braintrade-linear-softmax-v1`.

`POST /v1/learning/register` accepts the JSON artifact body. Core refuses registration unless the current contract is satisfied:

- feature and label schemas match;
- status is exactly `unreviewed_challenger` and `eligible_for_promotion=false`;
- target is supported at 5m, 15m or 30m;
- dataset identity is a valid lowercase SHA-256 and at least 600 post-purge rows were used;
- each recorded context split has at least a 30-row purge on both sides;
- `random_split=false`, `purged_chronological_split=true`, `automatic_promotion=false`;
- classes are exactly `[-1, 0, 1]`;
- feature names are unique and bounded;
- means, positive standard deviations, biases and all three weight rows have exact matching dimensions and finite values;
- train, validation and holdout metric summaries exist with finite values.

A valid artifact is stored as immutable compact JSON inside `learning_models`. Registration creates a unique model ID and makes that model the current `challenger` slot. Registering a new challenger may replace the challenger pointer, but it never changes a `champion` slot and never enables inference.

There is intentionally no Champion promotion endpoint in this checkpoint.

## 6. Next required layer

Before Champion promotion or runtime inference can exist, BrainTrade still needs a separate forward chronological evaluation record on data not used by the trainer. Promotion logic must require that evidence, immutable artifact identity and an explicit manual action. Shadow remains disconnected from every registered model until those conditions are implemented and reviewed.
