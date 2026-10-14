#!/usr/bin/env python3
"""BrainTrade offline baseline trainer.

Reads a Learning Foundation `dataset.jsonl` export and trains a deterministic,
interpretable 3-class softmax linear baseline. It never connects to MT5, never
places orders, never writes the runtime database and never promotes a model.

The split is chronological per account context with a 30-row purge around split
boundaries so labels that look 30 minutes into the future cannot overlap the
adjacent partition.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import sys
from collections import defaultdict

FEATURE_SCHEMA = "gold-m1-feature-v1"
LABEL_SCHEMA = "gold-m1-outcome-v1"
MODEL_SCHEMA = "braintrade-linear-softmax-v1"
CLASSES = (-1, 0, 1)
PURGE_ROWS = 30
PATTERNS = (
    "zone_rejection",
    "liquidity_sweep",
    "reversal",
    "breakout",
    "breakout_retest",
    "value_reclaim",
)
TIMEFRAMES = ("M1", "M5", "M15")


def finite(value, default=0.0):
    try:
        value = float(value)
    except (TypeError, ValueError):
        return default
    return value if math.isfinite(value) else default


def clip(value, low, high):
    return max(low, min(high, finite(value)))


def flag(value):
    return 1.0 if value is True else 0.0


def by_timeframe(items):
    result = {}
    for item in items or []:
        if isinstance(item, dict) and item.get("timeframe") in TIMEFRAMES:
            result[item["timeframe"]] = item
    return result


def flatten(features):
    if features.get("schema") != FEATURE_SCHEMA:
        raise ValueError(f"unsupported feature schema: {features.get('schema')!r}")
    values = {}

    def put(name, value):
        values[name] = finite(value)

    put("legacy_directional_score", clip(features.get("legacy_directional_score"), -1, 1))
    trend = features.get("trend") or {}
    put("trend.direction", clip(trend.get("direction"), -1, 1))
    put("trend.evidence", clip(trend.get("evidence"), 0, 1))
    put("trend.aligned", flag(trend.get("aligned")))

    regime = features.get("regime") or {}
    for name in ("direction", "structure_direction", "meta_v2_direction"):
        put(f"regime.{name}", clip(regime.get(name), -1, 1))
    for name in (
        "trend_evidence",
        "efficiency",
        "structure_evidence",
        "volatility_evidence",
        "meta_v2_evidence",
    ):
        put(f"regime.{name}", clip(regime.get(name), 0, 1))
    put("regime.volatility_ratio", clip(regime.get("volatility_ratio"), 0, 8))
    put("regime.tradable_trend", flag(regime.get("tradable_trend")))
    put("regime.structure_consensus", flag(regime.get("structure_consensus")))
    put("regime.volatility_shock", flag(regime.get("volatility_shock")))
    put("regime.meta_v2_eligible", flag(regime.get("meta_v2_eligible")))

    momentum = by_timeframe(features.get("momentum"))
    for tf in TIMEFRAMES:
        row = momentum.get(tf, {})
        put(f"momentum.{tf}.present", 1.0 if row else 0.0)
        put(f"momentum.{tf}.direction", clip(row.get("direction"), -1, 1))
        put(f"momentum.{tf}.roc3_atr", clip(row.get("roc3_atr"), -6, 6))
        put(f"momentum.{tf}.roc8_atr", clip(row.get("roc8_atr"), -10, 10))
        put(f"momentum.{tf}.acceleration_atr", clip(row.get("acceleration_atr"), -6, 6))
        put(f"momentum.{tf}.rsi_centered", clip((finite(row.get("rsi14"), 50.0) - 50.0) / 25.0, -2, 2))
        put(f"momentum.{tf}.strength", clip(row.get("strength"), 0, 1))
        put(f"momentum.{tf}.divergence", clip(row.get("divergence"), -1, 1))

    gann = by_timeframe(features.get("gann"))
    for tf in TIMEFRAMES:
        row = gann.get(tf, {})
        available = row.get("available") is True
        put(f"gann.{tf}.available", 1.0 if available else 0.0)
        put(f"gann.{tf}.direction", clip(row.get("direction"), -1, 1))
        put(f"gann.{tf}.strength", clip(row.get("strength"), 0, 1))
        put(f"gann.{tf}.atr_per_bar", clip(row.get("atr_per_bar"), -2, 2))
        put(f"gann.{tf}.distance_atr", clip(row.get("distance_atr"), 0, 10))
        put(f"gann.{tf}.eighth", clip(row.get("nearest_eighth"), 0, 8) / 8.0)

    zones = features.get("zones") or {}
    put("zones.active_count", clip(zones.get("active_count"), 0, 50) / 10.0)
    put("zones.nearest_present", 1.0 if zones.get("nearest_side") is not None else 0.0)
    put("zones.nearest_side", clip(zones.get("nearest_side"), -1, 1))
    put("zones.nearest_strength", clip(zones.get("nearest_strength"), 0, 1))
    put("zones.nearest_mid_distance_atr", clip(zones.get("nearest_mid_distance_atr"), -10, 10))
    put("zones.nearest_width_atr", clip(zones.get("nearest_width_atr"), 0, 10))

    setup = features.get("setup") or {}
    pattern = setup.get("top_pattern")
    put("setup.count", clip(setup.get("count"), 0, 6) / 6.0)
    put("setup.top_present", 1.0 if pattern else 0.0)
    put("setup.top_side", clip(setup.get("top_side"), -1, 1))
    put("setup.top_evidence", clip(setup.get("top_evidence"), 0, 1))
    for name in PATTERNS:
        put(f"setup.pattern.{name}", 1.0 if pattern == name else 0.0)

    activity = features.get("activity") or {}
    ratio = activity.get("tick_activity_ratio")
    put("activity.tick_present", 1.0 if ratio is not None else 0.0)
    put("activity.tick_ratio", clip(ratio, 0, 5) / 5.0)
    put("activity.profile_available", flag(activity.get("profile_available")))
    for name in ("poc_distance_atr", "value_low_distance_atr", "value_high_distance_atr"):
        put(f"activity.{name}", clip(activity.get(name), -10, 10))

    session = features.get("session") or {}
    put("session.covers_day_open", flag(session.get("covers_day_open")))
    for name in ("open_distance_atr", "high_distance_atr", "low_distance_atr", "vwap_distance_atr"):
        put(f"session.{name}", clip(session.get(name), -12, 12))
    put("session.vwap_present", 1.0 if session.get("vwap_distance_atr") is not None else 0.0)

    return values


def target_from_label(label, minutes):
    if label.get("schema") != LABEL_SCHEMA:
        raise ValueError(f"unsupported label schema: {label.get('schema')!r}")
    for horizon in label.get("horizons") or []:
        if horizon.get("minutes") == minutes:
            value = int(horizon.get("direction_0_2atr"))
            if value not in CLASSES:
                raise ValueError("invalid target class")
            return value
    raise ValueError(f"target horizon {minutes}m missing")


def load_dataset(path, minutes):
    dataset = path / "dataset.jsonl" if path.is_dir() else path
    raw = dataset.read_bytes()
    sha = hashlib.sha256(raw).hexdigest()
    rows = []
    for line_number, line in enumerate(raw.splitlines(), 1):
        if not line.strip():
            continue
        try:
            row = json.loads(line)
            if row.get("feature_schema") != FEATURE_SCHEMA or row.get("label_schema") != LABEL_SCHEMA:
                raise ValueError("schema mismatch")
            rows.append({
                "context": str(row["context"]),
                "bar_time": int(row["bar_time"]),
                "snapshot_id": int(row["snapshot_id"]),
                "x": flatten(row["features"]),
                "y": target_from_label(row["label"], minutes),
            })
        except Exception as exc:
            raise ValueError(f"dataset line {line_number}: {exc}") from exc
    if not rows:
        raise ValueError("dataset has no matured rows")
    return dataset, sha, rows


def split_purged(rows, min_rows):
    groups = defaultdict(list)
    for row in rows:
        groups[row["context"]].append(row)
    train, validation, holdout, metadata = [], [], [], []
    for context, group in sorted(groups.items()):
        group.sort(key=lambda row: (row["bar_time"], row["snapshot_id"]))
        # Duplicate timestamps would make split identity ambiguous even if snapshot IDs differ.
        if any(a["bar_time"] >= b["bar_time"] for a, b in zip(group, group[1:])):
            raise ValueError(f"context {context}: duplicate or non-increasing bar_time")
        n = len(group)
        cut_train = int(n * 0.60)
        cut_validation = int(n * 0.80)
        a = max(0, cut_train - PURGE_ROWS)
        b = min(n, cut_train + PURGE_ROWS)
        c = max(b, cut_validation - PURGE_ROWS)
        d = min(n, cut_validation + PURGE_ROWS)
        parts = (group[:a], group[b:c], group[d:])
        if min(map(len, parts)) < 10:
            continue
        train.extend(parts[0])
        validation.extend(parts[1])
        holdout.extend(parts[2])
        metadata.append({
            "context": context,
            "source_rows": n,
            "train_rows": len(parts[0]),
            "validation_rows": len(parts[1]),
            "holdout_rows": len(parts[2]),
            "purge_rows_each_side": PURGE_ROWS,
            "train_last_bar": parts[0][-1]["bar_time"],
            "validation_first_bar": parts[1][0]["bar_time"],
            "validation_last_bar": parts[1][-1]["bar_time"],
            "holdout_first_bar": parts[2][0]["bar_time"],
            "holdout_last_bar": parts[2][-1]["bar_time"],
        })
    total = len(train) + len(validation) + len(holdout)
    if total < min_rows:
        raise ValueError(
            f"insufficient post-purge rows: {total}; need at least {min_rows}. "
            "Collect more matured M1 snapshots before training."
        )
    if min(len(train), len(validation), len(holdout)) < 30:
        raise ValueError("train/validation/holdout partitions are too small")
    for name, part in (("train", train), ("validation", validation), ("holdout", holdout)):
        present = {row["y"] for row in part}
        if present != set(CLASSES):
            raise ValueError(f"{name} partition does not contain all three target classes: {sorted(present)}")
    return train, validation, holdout, metadata


def matrix(rows, names):
    return [[row["x"].get(name, 0.0) for name in names] for row in rows]


def standardizer(x):
    n = len(x)
    d = len(x[0])
    means = [sum(row[j] for row in x) / n for j in range(d)]
    stds = []
    for j in range(d):
        variance = sum((row[j] - means[j]) ** 2 for row in x) / max(1, n - 1)
        stds.append(max(math.sqrt(max(0.0, variance)), 1e-9))
    return means, stds


def transform(x, means, stds):
    return [[(value - means[j]) / stds[j] for j, value in enumerate(row)] for row in x]


def softmax(scores):
    top = max(scores)
    exp = [math.exp(max(-60.0, min(60.0, score - top))) for score in scores]
    total = sum(exp)
    return [value / total for value in exp]


def predict_proba(x, weights, biases):
    return softmax([
        biases[k] + sum(weight * value for weight, value in zip(weights[k], x))
        for k in range(len(CLASSES))
    ])


def evaluate(x, y, weights, biases):
    confusion = [[0 for _ in CLASSES] for _ in CLASSES]
    loss = 0.0
    confidence = 0.0
    correct = 0
    for row, truth in zip(x, y):
        probabilities = predict_proba(row, weights, biases)
        target = CLASSES.index(truth)
        prediction = max(range(len(CLASSES)), key=lambda k: probabilities[k])
        confusion[target][prediction] += 1
        correct += int(prediction == target)
        loss -= math.log(max(probabilities[target], 1e-15))
        confidence += max(probabilities)
    recalls, f1s = [], []
    for k in range(len(CLASSES)):
        tp = confusion[k][k]
        fn = sum(confusion[k]) - tp
        fp = sum(confusion[r][k] for r in range(len(CLASSES))) - tp
        recall = tp / (tp + fn) if tp + fn else 0.0
        precision = tp / (tp + fp) if tp + fp else 0.0
        f1 = 2 * precision * recall / (precision + recall) if precision + recall else 0.0
        recalls.append(recall)
        f1s.append(f1)
    return {
        "rows": len(y),
        "accuracy": correct / len(y),
        "balanced_accuracy": sum(recalls) / len(recalls),
        "macro_f1": sum(f1s) / len(f1s),
        "log_loss": loss / len(y),
        "mean_confidence": confidence / len(y),
        "confusion_truth_rows_pred_cols": confusion,
    }


def majority_baseline(y):
    counts = {value: y.count(value) for value in CLASSES}
    majority = max(CLASSES, key=lambda value: (counts[value], -abs(value)))
    accuracy = counts[majority] / len(y)
    return {"class": majority, "accuracy": accuracy, "counts": counts}


def train_softmax(x_train, y_train, x_validation, y_validation, epochs=80, learning_rate=0.025, l2=2e-4):
    d = len(x_train[0])
    weights = [[0.0] * d for _ in CLASSES]
    biases = [0.0] * len(CLASSES)
    counts = {value: y_train.count(value) for value in CLASSES}
    class_weight = {
        value: min(4.0, max(0.25, len(y_train) / (len(CLASSES) * counts[value])))
        for value in CLASSES
    }
    best = None
    patience = 18
    stale = 0
    for epoch in range(epochs):
        rate = learning_rate / math.sqrt(1.0 + epoch / 12.0)
        # Deterministic cyclic offset changes order without randomizing the chronological split.
        offset = (epoch * 97) % len(x_train)
        order = list(range(offset, len(x_train))) + list(range(0, offset))
        for index in order:
            row = x_train[index]
            truth = y_train[index]
            target = CLASSES.index(truth)
            probabilities = predict_proba(row, weights, biases)
            scale = class_weight[truth]
            for k in range(len(CLASSES)):
                error = (probabilities[k] - (1.0 if k == target else 0.0)) * scale
                biases[k] -= rate * error
                for j, value in enumerate(row):
                    weights[k][j] -= rate * (error * value + l2 * weights[k][j])
        metrics = evaluate(x_validation, y_validation, weights, biases)
        if best is None or metrics["log_loss"] < best[0] - 1e-7:
            best = (
                metrics["log_loss"],
                [row[:] for row in weights],
                biases[:],
                epoch + 1,
                metrics,
            )
            stale = 0
        else:
            stale += 1
            if stale >= patience:
                break
    _, weights, biases, best_epoch, validation_metrics = best
    return weights, biases, best_epoch, validation_metrics, class_weight


def main():
    parser = argparse.ArgumentParser(description="Train BrainTrade deterministic linear baseline")
    parser.add_argument("dataset", type=Path, help="learning export folder or dataset.jsonl")
    parser.add_argument("--output", type=Path, help="output JSON artifact")
    parser.add_argument("--target-minutes", type=int, choices=(5, 15, 30), default=15)
    parser.add_argument("--min-rows", type=int, default=600)
    args = parser.parse_args()

    dataset, dataset_sha, rows = load_dataset(args.dataset, args.target_minutes)
    train, validation, holdout, split_metadata = split_purged(rows, args.min_rows)
    feature_names = sorted({name for row in train for name in row["x"]})
    if not feature_names:
        raise ValueError("no training features")

    raw_train = matrix(train, feature_names)
    raw_validation = matrix(validation, feature_names)
    raw_holdout = matrix(holdout, feature_names)
    means, stds = standardizer(raw_train)
    x_train = transform(raw_train, means, stds)
    x_validation = transform(raw_validation, means, stds)
    x_holdout = transform(raw_holdout, means, stds)
    y_train = [row["y"] for row in train]
    y_validation = [row["y"] for row in validation]
    y_holdout = [row["y"] for row in holdout]

    weights, biases, epoch, validation_metrics, class_weight = train_softmax(
        x_train, y_train, x_validation, y_validation
    )
    train_metrics = evaluate(x_train, y_train, weights, biases)
    holdout_metrics = evaluate(x_holdout, y_holdout, weights, biases)
    baseline = {
        "train": majority_baseline(y_train),
        "validation": majority_baseline(y_validation),
        "holdout": majority_baseline(y_holdout),
    }

    artifact = {
        "schema": MODEL_SCHEMA,
        "status": "unreviewed_challenger",
        "eligible_for_promotion": False,
        "feature_schema": FEATURE_SCHEMA,
        "label_schema": LABEL_SCHEMA,
        "target": f"{args.target_minutes}m.direction_0_2atr",
        "dataset": {
            "file": dataset.name,
            "sha256": dataset_sha,
            "source_rows": len(rows),
            "used_rows": len(train) + len(validation) + len(holdout),
            "purge_rows_each_side": PURGE_ROWS,
            "split": split_metadata,
        },
        "model": {
            "classes": list(CLASSES),
            "feature_names": feature_names,
            "means": means,
            "stds": stds,
            "weights": weights,
            "biases": biases,
        },
        "training": {
            "algorithm": "deterministic online softmax linear regression",
            "best_epoch": epoch,
            "class_weight": {str(key): value for key, value in class_weight.items()},
            "random_split": False,
            "purged_chronological_split": True,
            "automatic_promotion": False,
        },
        "metrics": {
            "train": train_metrics,
            "validation": validation_metrics,
            "holdout": holdout_metrics,
            "majority_baseline": baseline,
        },
        "limitations": [
            "This is a research baseline, not a profitability claim.",
            "No quote, DOM or news features are present because historical replay parity is not established for them.",
            "Holdout metrics must be reviewed before any registry promotion.",
            "The artifact is not executable by Shadow and cannot place broker orders.",
        ],
    }
    output = args.output or dataset.with_name(f"linear-baseline-{args.target_minutes}m.json")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(artifact, ensure_ascii=False, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    print(json.dumps({
        "artifact": str(output),
        "dataset_sha256": dataset_sha,
        "features": len(feature_names),
        "train_rows": len(train),
        "validation_rows": len(validation),
        "holdout_rows": len(holdout),
        "best_epoch": epoch,
        "validation_macro_f1": validation_metrics["macro_f1"],
        "holdout_macro_f1": holdout_metrics["macro_f1"],
        "promotion": "disabled",
    }, indent=2))


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(f"training refused: {exc}", file=sys.stderr)
        raise SystemExit(2)
