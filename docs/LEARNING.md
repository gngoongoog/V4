# BrainTrade Learning Foundation

This document describes the current learning pipeline on `development`. It is a research data pipeline, not an autonomous trading AI.

## Safety boundary

Current invariants:

- `training_enabled = false` inside the live Core.
- `prediction_enabled = false` inside Shadow.
- `live_self_modification = false`.
- No learned artifact can submit, modify or close an MT5 order.
- Legacy Meta remains the live Shadow decision source.

A high historical score is not a profitability guarantee and is not enough to promote a model.

## 1. Closed-bar feature capture

Core stores one immutable `learning_snapshots` row per account/server/symbol/currency context and completed M1 bar.

Feature schema: `gold-m1-feature-v1`.

Included inputs are reproducible from completed candle history:

- Trend Brain and frame evidence.
- Regime, Structure and Volatility Brain outputs.
- Meta-v2 candidate state as an input feature only.
- M1/M5/M15 momentum, RSI-derived state and divergence.
- Gann-inspired normalized geometry.
- Confirmed zone geometry and top setup metadata.
- MT5 candle tick-activity and approximate profile proxies.
- Current UTC session geometry derived from saved candles.

Deliberately excluded from v1 learning features:

- current bid/ask entry projection;
- live DOM / Market Book;
- News/Macro context;
- any future candle or future-derived statistic.

Those live feeds are excluded until historical replay parity and timestamp alignment are implemented for them.

## 2. Delayed outcome labels

Label schema: `gold-m1-outcome-v1`.

A label is not written when the feature is captured. Core waits until exact future M1 bars exist for 5, 15 and 30 minutes. Each horizon stores:

- final return in units of the reference ATR;
- maximum favorable excursion (MFE) in ATR;
- maximum adverse excursion (MAE) in ATR;
- direction class: `-1`, `0`, `1`, using ±0.20 ATR around zero.

If a required minute is missing, duplicated, out of order or gapped, the label is not shifted to another bar and is not fabricated.

## 3. Dataset export

`POST /v1/learning/export` writes only matured feature+label rows to `exports/learning-dataset-<timestamp>/dataset.jsonl` with deterministic database ordering.

`manifest.json` records the feature schema, label schema, row count and scope. No model artifact is included.

`GET /v1/learning` reports snapshot counts, matured/pending label counts, registry slots and explicit training/prediction disabled flags.

## 4. Offline baseline trainer

Run only after enough matured data exists:

```text
python scripts/train_linear_baseline.py <learning-export-folder>
```

The trainer has no network, MT5 or Core dependency. It reads `dataset.jsonl` and writes a JSON research artifact.

Current baseline:

- deterministic 3-class softmax linear model;
- default target: 15-minute direction class;
- fixed, transparent feature flattening;
- train-only standardization;
- class weighting from the train partition only;
- no random split;
- no automatic parameter search;
- no automatic model promotion.

### Purged chronological split

Each account context is sorted chronologically and split approximately 60% / 20% / 20% into Train / Validation / Holdout.

Because labels can inspect 30 future minutes, the trainer removes a 30-row purge on each side of split boundaries. This prevents a training label horizon from overlapping a validation/holdout feature period.

Contexts too short to keep usable post-purge partitions are excluded. Training is refused if the resulting dataset is too small or if any partition lacks one of the three classes.

## 5. Metrics

The artifact records, separately for train, validation and holdout:

- accuracy;
- balanced accuracy;
- macro F1;
- multiclass log loss;
- mean confidence;
- confusion matrix;
- majority-class baseline.

These metrics describe the labelled dataset only. They are not expected profit, win rate or broker execution quality.

## 6. Model registry

Core already creates `learning_models` and `learning_slots` tables. Slots are reserved for `champion` and `challenger`.

At this checkpoint no registration/promotion endpoint is enabled and the default registry is empty. A trainer artifact is always emitted as `unreviewed_challenger` with `eligible_for_promotion = false`.

The next registry increment must add artifact integrity verification, immutable metadata, chronological evaluation requirements and manual promotion. Inference must remain disconnected from Shadow until that work is complete and reviewed.
