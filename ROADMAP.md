# BrainTrade OS — Roadmap

This file separates near-term work from long-term ideas. `docs/ARCHITECTURE.md` is the broader design document and must not be treated as a completed-feature list.

## P0 — finish current 0.4.0 checkpoint

1. Complete KTE unit/regression tests.
2. Validate stateful partial exits, break-even, trailing and cooldown edge cases.
3. Verify KTE chart/report/export output uses the same saved trade ledger.
4. Update validation artifacts and release status to match the actual KTE implementation.
5. Run full Rust + Python test suites in a Rust-capable environment.

## P1 — broker/laptop validation

1. Validate native Windows build on the user's laptop.
2. Verify MT5 historical retrieval against the user's broker/account.
3. Confirm UI behavior on all pages and timeframes.
4. Test Telegram delivery only after the user supplies credentials locally; never commit tokens.

## P2 — analysis expansion

- Gann research brain.
- Order-flow/DOM acquisition when the broker/feed exposes real data.
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
