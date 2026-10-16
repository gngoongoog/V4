#!/usr/bin/env python3
"""Deterministic BrainTrade feature-group ablation research.

This tool consumes the immutable Learning Foundation ``dataset.jsonl`` export and
reuses the exact feature flattener, purged chronological split and softmax
trainer from ``train_linear_baseline.py``.  It retrains a fresh model after
removing one feature group at a time and compares validation/holdout metrics
against the full model.

The analysis is research-only.  It never connects to MT5, never writes the
runtime database, never changes inference, and never promotes a model.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
TRAINER_PATH = HERE / "train_linear_baseline.py"


def _load_trainer():
    spec = importlib.util.spec_from_file_location("braintrade_ablation_baseline", TRAINER_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load train_linear_baseline.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


trainer = _load_trainer()

SCHEMA = "braintrade-feature-ablation-v1"
DEFAULT_CORRELATION_THRESHOLD = 0.95
DEFAULT_MAX_CORRELATION_PAIRS = 50

# Prefixes intentionally mirror the feature namespaces emitted by the shared
# baseline flattener.  Keeping this mapping here makes the scientific grouping
# explicit and reviewable instead of inferring groups from model weights.
GROUP_PREFIXES = {
    "legacy": ("legacy_directional_score",),
    "trend": ("trend.",),
    "regime": ("regime.",),
    "momentum": ("momentum.",),
    "gann": ("gann.",),
    "zones": ("zones.",),
    "setup": ("setup.",),
    "activity": ("activity.",),
    "session": ("session.",),
}


def feature_group(name: str) -> str:
    for group, prefixes in GROUP_PREFIXES.items():
        for prefix in prefixes:
            if name == prefix or name.startswith(prefix):
                return group
    return "ungrouped"


def grouped_features(feature_names):
    grouped = {name: [] for name in GROUP_PREFIXES}
    grouped["ungrouped"] = []
    for feature in feature_names:
        grouped[feature_group(feature)].append(feature)
    return grouped


def _fit(train, validation, holdout, feature_names, epochs):
    if not feature_names:
        raise ValueError("ablation removed every feature")
    raw_train = trainer.matrix(train, feature_names)
    raw_validation = trainer.matrix(validation, feature_names)
    raw_holdout = trainer.matrix(holdout, feature_names)
    means, stds = trainer.standardizer(raw_train)
    x_train = trainer.transform(raw_train, means, stds)
    x_validation = trainer.transform(raw_validation, means, stds)
    x_holdout = trainer.transform(raw_holdout, means, stds)
    y_train = [row["y"] for row in train]
    y_validation = [row["y"] for row in validation]
    y_holdout = [row["y"] for row in holdout]

    weights, biases, best_epoch, validation_metrics, class_weight = trainer.train_softmax(
        x_train,
        y_train,
        x_validation,
        y_validation,
        epochs=epochs,
    )
    return {
        "feature_count": len(feature_names),
        "best_epoch": best_epoch,
        "class_weight": {str(key): value for key, value in class_weight.items()},
        "metrics": {
            "train": trainer.evaluate(x_train, y_train, weights, biases),
            "validation": validation_metrics,
            "holdout": trainer.evaluate(x_holdout, y_holdout, weights, biases),
        },
    }


def _delta(ablated, full):
    return {
        "accuracy": ablated["accuracy"] - full["accuracy"],
        "balanced_accuracy": ablated["balanced_accuracy"] - full["balanced_accuracy"],
        "macro_f1": ablated["macro_f1"] - full["macro_f1"],
        # Positive log-loss delta means removing the group made probability
        # quality worse, which is the opposite direction of the score metrics.
        "log_loss": ablated["log_loss"] - full["log_loss"],
        "mean_confidence": ablated["mean_confidence"] - full["mean_confidence"],
    }


def _effect_label(validation_delta, holdout_delta):
    val = validation_delta["macro_f1"]
    hold = holdout_delta["macro_f1"]
    if val <= -0.01 and hold <= -0.01:
        return "removal_hurt_both"
    if val >= 0.01 and hold >= 0.01:
        return "removal_helped_both"
    if abs(val) < 0.01 and abs(hold) < 0.01:
        return "small_or_redundant_effect"
    return "mixed_needs_more_evidence"


def _pearson(left, right):
    n = len(left)
    if n != len(right) or n < 3:
        return None
    left_mean = sum(left) / n
    right_mean = sum(right) / n
    left_ss = sum((value - left_mean) ** 2 for value in left)
    right_ss = sum((value - right_mean) ** 2 for value in right)
    if left_ss <= 1e-18 or right_ss <= 1e-18:
        return None
    cross = sum((a - left_mean) * (b - right_mean) for a, b in zip(left, right))
    value = cross / math.sqrt(left_ss * right_ss)
    if not math.isfinite(value):
        return None
    return max(-1.0, min(1.0, value))


def high_cross_group_correlations(train, feature_names, threshold, max_pairs):
    if not 0.0 < threshold <= 1.0:
        raise ValueError("correlation threshold must be in (0,1]")
    if max_pairs < 1:
        raise ValueError("max correlation pairs must be positive")
    columns = {
        name: [row["x"].get(name, 0.0) for row in train]
        for name in feature_names
    }
    pairs = []
    group_counts = {group: 0 for group in GROUP_PREFIXES}
    group_counts["ungrouped"] = 0
    for index, left_name in enumerate(feature_names):
        left_group = feature_group(left_name)
        for right_name in feature_names[index + 1 :]:
            right_group = feature_group(right_name)
            if left_group == right_group:
                continue
            correlation = _pearson(columns[left_name], columns[right_name])
            if correlation is None or abs(correlation) < threshold:
                continue
            pairs.append(
                {
                    "left": left_name,
                    "left_group": left_group,
                    "right": right_name,
                    "right_group": right_group,
                    "correlation": correlation,
                }
            )
            group_counts[left_group] += 1
            group_counts[right_group] += 1
    pairs.sort(key=lambda row: (-abs(row["correlation"]), row["left"], row["right"]))
    return {
        "threshold": threshold,
        "total_pairs": len(pairs),
        "reported_pairs": pairs[:max_pairs],
        "cross_group_pair_memberships": group_counts,
        "constant_features_ignored": True,
    }


def analyze(dataset_path, target_minutes=15, min_rows=600, epochs=80,
            groups=None, correlation_threshold=DEFAULT_CORRELATION_THRESHOLD,
            max_correlation_pairs=DEFAULT_MAX_CORRELATION_PAIRS):
    if epochs < 1 or epochs > 400:
        raise ValueError("epochs must be between 1 and 400")
    dataset, dataset_sha, rows = trainer.load_dataset(dataset_path, target_minutes)
    train, validation, holdout, split_metadata = trainer.split_purged(rows, min_rows)
    feature_names = sorted({name for row in train for name in row["x"]})
    if not feature_names:
        raise ValueError("no training features")
    grouped = grouped_features(feature_names)
    if grouped["ungrouped"]:
        raise ValueError(f"unassigned feature namespace(s): {grouped['ungrouped']}")

    selected_groups = list(GROUP_PREFIXES) if groups is None else list(groups)
    if not selected_groups:
        raise ValueError("at least one ablation group is required")
    unknown = [group for group in selected_groups if group not in GROUP_PREFIXES]
    if unknown:
        raise ValueError(f"unknown ablation group(s): {unknown}")
    if len(set(selected_groups)) != len(selected_groups):
        raise ValueError("duplicate ablation groups")

    full = _fit(train, validation, holdout, feature_names, epochs)
    ablations = []
    for group in selected_groups:
        removed = grouped[group]
        if not removed:
            raise ValueError(f"ablation group has no features: {group}")
        kept = [name for name in feature_names if name not in set(removed)]
        model = _fit(train, validation, holdout, kept, epochs)
        validation_delta = _delta(model["metrics"]["validation"], full["metrics"]["validation"])
        holdout_delta = _delta(model["metrics"]["holdout"], full["metrics"]["holdout"])
        ablations.append(
            {
                "group": group,
                "removed_feature_count": len(removed),
                "removed_features": removed,
                "remaining_feature_count": len(kept),
                "best_epoch": model["best_epoch"],
                "metrics": model["metrics"],
                "delta_vs_full": {
                    "validation": validation_delta,
                    "holdout": holdout_delta,
                },
                "effect_label": _effect_label(validation_delta, holdout_delta),
            }
        )

    redundancy = high_cross_group_correlations(
        train,
        feature_names,
        correlation_threshold,
        max_correlation_pairs,
    )
    identity_payload = {
        "schema": SCHEMA,
        "dataset_sha256": dataset_sha,
        "target_minutes": target_minutes,
        "feature_schema": trainer.FEATURE_SCHEMA,
        "label_schema": trainer.LABEL_SCHEMA,
        "groups": selected_groups,
        "epochs": epochs,
        "correlation_threshold": correlation_threshold,
    }
    canonical = json.dumps(identity_payload, sort_keys=True, separators=(",", ":"), allow_nan=False)
    analysis_id = hashlib.sha256(canonical.encode("utf-8")).hexdigest()

    return {
        "schema": SCHEMA,
        "analysis_id": analysis_id,
        "status": "research_only",
        "eligible_for_promotion": False,
        "automatic_promotion": False,
        "prediction_enabled": False,
        "broker_effect": False,
        "feature_schema": trainer.FEATURE_SCHEMA,
        "label_schema": trainer.LABEL_SCHEMA,
        "target": f"{target_minutes}m.direction_0_2atr",
        "dataset": {
            "file": dataset.name,
            "sha256": dataset_sha,
            "source_rows": len(rows),
            "used_rows": len(train) + len(validation) + len(holdout),
            "purge_rows_each_side": trainer.PURGE_ROWS,
            "split": split_metadata,
        },
        "method": {
            "name": "leave-one-feature-group-out retraining",
            "shared_baseline_trainer": "braintrade-linear-softmax-v1",
            "random_split": False,
            "purged_chronological_split": True,
            "epochs_limit": epochs,
            "group_order": selected_groups,
            "holdout_used_for_training": False,
            "note": "Ablation is diagnostic evidence, not a feature-selection or promotion command.",
        },
        "feature_groups": {group: grouped[group] for group in GROUP_PREFIXES},
        "full_model": full,
        "ablations": ablations,
        "redundancy": redundancy,
        "limitations": [
            "One dataset cannot establish durable feature usefulness or profitability.",
            "Repeated ablation comparisons expose the holdout to diagnostic inspection; do not tune or promote from this artifact alone.",
            "High feature correlation does not prove causal redundancy.",
            "Ablation never changes BrainTrade execution, risk, Shadow inference, or model registry state.",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description="BrainTrade deterministic feature-group ablation research")
    parser.add_argument("dataset", type=Path, help="learning export folder or dataset.jsonl")
    parser.add_argument("--output", type=Path, help="output JSON artifact")
    parser.add_argument("--target-minutes", type=int, choices=(5, 15, 30), default=15)
    parser.add_argument("--min-rows", type=int, default=600)
    parser.add_argument("--epochs", type=int, default=80)
    parser.add_argument(
        "--groups",
        help="comma-separated subset of: " + ",".join(GROUP_PREFIXES),
    )
    parser.add_argument("--correlation-threshold", type=float, default=DEFAULT_CORRELATION_THRESHOLD)
    parser.add_argument("--max-correlation-pairs", type=int, default=DEFAULT_MAX_CORRELATION_PAIRS)
    args = parser.parse_args()
    groups = None
    if args.groups is not None:
        groups = [value.strip() for value in args.groups.split(",") if value.strip()]
    artifact = analyze(
        args.dataset,
        target_minutes=args.target_minutes,
        min_rows=args.min_rows,
        epochs=args.epochs,
        groups=groups,
        correlation_threshold=args.correlation_threshold,
        max_correlation_pairs=args.max_correlation_pairs,
    )
    dataset = args.dataset / "dataset.jsonl" if args.dataset.is_dir() else args.dataset
    output = args.output or dataset.with_name(f"feature-ablation-{args.target_minutes}m.json")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(artifact, ensure_ascii=False, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    print(
        json.dumps(
            {
                "artifact": str(output),
                "analysis_id": artifact["analysis_id"],
                "dataset_sha256": artifact["dataset"]["sha256"],
                "groups": len(artifact["ablations"]),
                "features": artifact["full_model"]["feature_count"],
                "holdout_macro_f1": artifact["full_model"]["metrics"]["holdout"]["macro_f1"],
                "cross_group_high_correlations": artifact["redundancy"]["total_pairs"],
                "promotion": "disabled",
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(f"ablation refused: {exc}", file=sys.stderr)
        raise SystemExit(2)
