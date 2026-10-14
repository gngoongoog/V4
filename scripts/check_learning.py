"""End-to-end learning review + read-only inference check against the real Core.
Uses synthetic history only. Never enables broker execution or model influence on Shadow.
"""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import time

ROOT = Path(__file__).resolve().parents[1]
TRAINER = ROOT / "scripts" / "train_linear_baseline.py"
FIXTURE = ROOT / "tests" / "learning_feature_parity.json"


def _trainer():
    spec = importlib.util.spec_from_file_location("braintrade_train_linear_baseline_e2e", TRAINER)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def _wait(api, path, wanted="completed", timeout=30.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        state = api(path)
        if state["state"] == wanted:
            return state
        assert state["state"] != "error", state
        time.sleep(0.025)
    raise AssertionError(f"Timed out waiting for {path}: {wanted}")


def _evaluation_source(api, snapshot):
    request = dict(
        strategy="meta_v1",
        from_date="2024-11-01",
        to_date="2024-11-05",
        initial_equity=10000.0,
        spread_floor=0.02,
    )
    api("/v1/backtest/start", request)
    origin = None
    while True:
        job = api("/v1/gateway/history/request")["job"]
        if job is None:
            break
        if origin is None:
            origin = job["from"]
        bars = []
        for second in range(job["from"], job["to"], 60):
            index = (second - origin) // 60
            wave = ((index % 180) - 90) * 0.002
            price = 2100.0 + index * 0.004 + wave
            bars.append(
                dict(
                    time=second,
                    open=price,
                    high=price + 0.22,
                    low=price - 0.18,
                    close=price + 0.05,
                    tick_volume=90 + index % 35,
                    spread_points=2,
                )
            )
        api("/v1/gateway/history/chunk", dict(job, bars=bars, error=""))
    return _wait(api, "/v1/backtest")["latest_id"]


def _chronological_evidence(api, source_id):
    started = api(
        "/v1/evaluation/start",
        {"source_id": source_id, "plan": {"mode": "holdout", "holdout_percent": 30}},
    )
    assert started["prediction_enabled"] is False
    assert started["promotion_enabled"] is False
    state = _wait(api, "/v1/evaluation")
    evaluation_id = state["latest_id"]
    report = api("/v1/evaluation/report", {"id": evaluation_id})
    assert report["source_id"] == source_id
    assert report["immutable"] is True
    assert report["prediction_enabled"] is False
    assert report["promotion_enabled"] is False
    identity = report["source_identity"]
    assert identity["algorithm"].startswith("meta-v1")
    assert identity["strategy"] == "meta_v1"
    roles = {row["window"]["role"] for row in report["evaluation"]["windows"]}
    assert roles == {"development", "holdout"}
    return evaluation_id


def _artifact(snapshot):
    trainer = _trainer()
    fixture = json.loads(FIXTURE.read_text(encoding="utf-8"))
    feature_names = sorted(trainer.flatten(fixture["features"]))
    assert len(feature_names) == fixture["expected_feature_count"] == 87
    context = json.dumps(
        [snapshot["server"], snapshot["login"], snapshot["symbol"], snapshot["currency"]],
        separators=(",", ":"),
    )
    metric = dict(
        rows=100,
        accuracy=0.40,
        balanced_accuracy=0.40,
        macro_f1=0.40,
        log_loss=1.0,
        mean_confidence=0.50,
    )
    dimension = len(feature_names)
    return {
        "schema": "braintrade-linear-softmax-v1",
        "status": "unreviewed_challenger",
        "eligible_for_promotion": False,
        "feature_schema": "gold-m1-feature-v1",
        "label_schema": "gold-m1-outcome-v1",
        "target": "15m.direction_0_2atr",
        "dataset": {
            "sha256": "b" * 64,
            "used_rows": 900,
            "split": [
                {
                    "context": context,
                    "source_rows": 1000,
                    "train_rows": 570,
                    "validation_rows": 170,
                    "holdout_rows": 160,
                    "purge_rows_each_side": 30,
                    "train_last_bar": 1,
                    "validation_first_bar": 2,
                    "validation_last_bar": 3,
                    "holdout_first_bar": 4,
                    "holdout_last_bar": 5,
                }
            ],
        },
        "model": {
            "classes": [-1, 0, 1],
            "feature_names": feature_names,
            "means": [0.0] * dimension,
            "stds": [1.0] * dimension,
            "weights": [[0.0] * dimension for _ in range(3)],
            "biases": [-1.0, 0.0, 1.0],
        },
        "training": {
            "algorithm": "synthetic parity fixture only",
            "random_split": False,
            "purged_chronological_split": True,
            "automatic_promotion": False,
        },
        "metrics": {"train": metric, "validation": metric, "holdout": metric},
        "limitations": ["CI synthetic artifact only; not a profitability result."],
    }


def check(api, snapshot):
    source_id = _evaluation_source(api, snapshot)
    evaluation_id = _chronological_evidence(api, source_id)

    registered = api("/v1/learning/register", _artifact(snapshot))
    model_id = registered["model_id"]
    assert registered["slot"] == "challenger"
    assert registered["prediction_enabled"] is False
    assert registered["eligible_for_promotion"] is False

    eligibility = api("/v1/learning/review/status")
    assert eligibility["model_id"] == model_id
    assert eligibility["eligible_for_manual_review"] is True, eligibility
    assert eligibility["eligible_for_promotion"] is False
    assert eligibility["prediction_enabled"] is False
    assert eligibility["evidence_ids"] == [evaluation_id]

    review = api(
        "/v1/learning/review/create",
        {
            "model_id": model_id,
            "evaluation_ids": [evaluation_id],
            "note": "synthetic CI prerequisite review",
        },
    )
    review_id = review["id"]
    assert review["immutable"] is True
    assert review["eligible_for_promotion"] is False
    assert review["prediction_enabled"] is False

    inference = api("/v1/learning/inference")
    assert inference["read_only_gate_ready"] is True
    assert inference["current_model_id"] == model_id
    assert inference["review_record_id"] == review_id
    assert inference["predictions"] == 0
    assert inference["shadow_effect"] is False
    assert inference["risk_effect"] is False
    assert inference["broker_effect"] is False

    before = api("/v1/status")
    api("/v1/gateway/snapshot", snapshot)
    after = api("/v1/status")
    recent = api("/v1/learning/inference/recent")["predictions"]
    assert len(recent) == 1, recent
    prediction = recent[0]
    assert prediction["model_id"] == model_id
    assert prediction["review_record_id"] == review_id
    assert prediction["predicted_class"] == 1
    assert prediction["confidence"] > 0.65
    probabilities = prediction["probabilities"]
    assert probabilities["up"] > probabilities["flat"] > probabilities["down"]
    assert abs(sum(probabilities.values()) - 1.0) < 1e-12
    assert prediction["shadow_effect"] is False

    api("/v1/gateway/snapshot", snapshot)
    inference = api("/v1/learning/inference")
    assert inference["predictions"] == 1
    assert before["analysis"]["action"] == after["analysis"]["action"]
    assert after["execution_enabled"] is False

    learning = api("/v1/learning")
    assert learning["manual_review_eligibility"]["eligible_for_manual_review"] is True
    assert learning["read_only_inference"]["predictions"] == 1
    assert learning["champion"] is None
    assert learning["prediction_enabled"] is False
    assert learning["champion_promotion_enabled"] is False

    return {
        "source_id": source_id,
        "evaluation_id": evaluation_id,
        "model_id": model_id,
        "review_id": review_id,
    }
