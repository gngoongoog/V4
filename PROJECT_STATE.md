# BrainTrade OS — Project State

## Current checkpoint

- Development checkpoint: **0.4.0 in progress**.
- Last validated baseline: **0.3.0 source preview**.
- Last delivered Windows portable: **0.2.0 Preview**.
- Windows packaging remains deferred until Mohammed explicitly asks for a new build.
- Canonical repository: `gngoongoog/BrainTrade-OS`.
- `main` is the latest safe/resumable checkpoint.
- `development` is reserved for ongoing/WIP checkpoints.

## Implemented and preserved

- Native Rust Core + Slint desktop UI.
- Read-only MT5 gateway/bootstrap and account visibility.
- Start / Stop / reconnect / full-exit lifecycle controls.
- M1/M5/M15 completed-bar Meta analysis and shadow simulation.
- Persistent risk/shadow state and settings.
- Historical Meta/CES lab with asynchronous MT5 history collection.
- UTC chunking, account isolation, cancellation, raw-input/report persistence.
- Deterministic offline replay and same-data strategy experiments.
- CSV / JSON / HTML exports with an equity curve.
- Official Fed/ECB RSS cache, freshness/error state and manual refresh.
- Telegram read-only `/news` and `/backtest` handlers.

## 0.4.0 work already present

- KTE signal port based on the user's supplied KTE defaults.
- Stateful KTE historical execution path.
- Partial-target handling (TP1/TP2/TP3), break-even/trailing behavior and cooldown logic.
- KTE appears in the historical strategy selector.

## Still in progress for 0.4.0

- Final KTE validation and regression coverage.
- Final chart/export integration review for KTE results.
- Reconcile all docs/validation artifacts with the new KTE state.
- Native Windows/broker validation after the source checkpoint is stable.

## Explicitly not complete

Broker order execution/protection, trained ML/ONNX, Gann brain, true order-flow/DOM,
volume profile, news-informed trade decisions, walk-forward optimization, portfolio
risk manager and full restore remain roadmap items unless later commits say otherwise.

## Verification at this checkpoint

- Python gateway/packaging suite: **16/16 passed** on 2026-10-13 in the ChatGPT build environment.
- Rust tests were not rerun during the GitHub migration because Rust tooling is not installed in that environment.
- The prior 0.3.0 checkpoint records 18 domain + 13 runtime Rust tests as passed; treat that as prior validation, not a fresh 0.4.0 result.

## Continuation rule

GitHub is the source of truth. Before new work, read `CONTINUE_HERE.md`, `BUILD_HANDOFF.md`, this file, `ROADMAP.md`, `CHANGELOG.md`, `docs/CONTINUITY_PROTOCOL.md`, and recent commits.

Commit ongoing work in small coherent checkpoints to `development`. Keep `BUILD_HANDOFF.md` current with the exact task, changed areas, test state and next action. Promote only safe/resumable checkpoints to `main`. This rule exists specifically so an interrupted or quota-limited ChatGPT session cannot erase meaningful build progress.
