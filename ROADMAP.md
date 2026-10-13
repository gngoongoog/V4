# BrainTrade OS — Roadmap

This file separates near-term work from long-term ideas. `docs/ARCHITECTURE.md` is the broader design document and must not be treated as a completed-feature list.

## P0 — finish current 0.4.0 checkpoint

Completed on `development`:

1. KTE unit/regression coverage for partial sizing and trade management.
2. Stateful TP1/TP2/TP3, break-even, trailing, time-limit, ambiguity and cooldown validation.
3. KTE report/chart/export consistency from the same saved `Trade` ledger.
4. Detailed fills / stop-updates / targets CSV exports plus deterministic offline replay checks.
5. Fresh Linux Rust + Python + real-core integration CI.

Remaining before promotion to `main`:

1. Wait for the current Windows CI packaging job to complete; fix any real failure if present.
2. Reconcile final validation/release documents with that Windows result.
3. Promote the tested resumable 0.4.0 checkpoint to `main` only after the latest branch head is green.

## P1 — broker/laptop validation

1. Validate native Windows build on the user's laptop.
2. Verify MT5 historical retrieval against the user's broker/account.
3. Confirm UI behavior on all pages and supported analysis timeframes.
4. Test Telegram delivery only after credentials are supplied locally; never commit tokens.

## P2 — analysis expansion

- Gann research brain.
- Order-flow/DOM acquisition only when the broker/feed exposes real market depth/trade data.
- Volume-profile research module.
- Trend/regime brain and stronger Meta decision layer.
- Historical news/event replay and macro context.

## P3 — learning/research infrastructure

- Walk-forward evaluation with leakage controls.
- Model registry and reproducible experiment metadata.
- Trained ML/ONNX candidates only after robust baselines and validation.
- Portfolio/correlation risk research.

## P4 — execution, only after explicit validation

- Broker execution state machine.
- Idempotent order intents and reconciliation.
- Hard risk protections and account-mode handling.
- Partial-fill and reconnect recovery fixtures.

No roadmap item is considered implemented until code, tests and `PROJECT_STATE.md` agree.
