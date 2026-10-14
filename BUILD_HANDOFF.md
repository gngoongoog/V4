# BrainTrade OS — Build Handoff

## Active checkpoint — 2026-10-14

Canonical repository: `gngoongoog/V4`. Active branch: `development`. `main` remains an older deliberately promoted checkpoint.

Current release candidate: **0.4.0 Preview**.

## Validated executable/code boundary

Executable checkpoint: `74e6387e3f66b8035f5ce55db92561ec1809ebf4` (`Fix challenger performance review fixtures and tighten validation`).

Research workflow `38586058214`: **PASS** — Rust domain/runtime tests, Core build, Python tests, real-Core smoke, browser report interaction checks, native Desktop compile, all-page headless render and artifact upload completed successfully.

Windows workflow `38586058215`: **PASS** — Linux duplicate validation, Windows workspace/Python tests, release Desktop/Core/gateway portable build, packaged runtime checks, native render check and artifact upload completed successfully.

Windows artifact: `BrainTrade-Windows-x64-v0.4.0-Preview`, 33,229,148 bytes, SHA-256 `e9e09749359a8bfd4cc14959d141d1c8485bf3fe427887413f4b2c5274db0237`.

A previous candidate run exposed a test-fixture calibration mismatch (the “good” fixture used confidence 0.80 while the policy ECE ceiling was 0.12). The fixture was corrected at the validated checkpoint; the final policy tests and both CI workflows pass.

Documentation commits after this checkpoint do not change executable behavior and are not a substitute for native acceptance on the user's own MT5/broker environment.

## What was completed from the prior handoff

The prior exact next action was the **Challenger Performance Review Policy**. It is now implemented in `crates/runtime/src/learning_performance_review.rs`, wired into Core and exposed on the existing Learning page.

Policy: `challenger-performance-review-v1`.

Fixed evidence gates include:

- minimum total scored samples: 600;
- minimum realized support per class: 60;
- minimum samples per context: 150;
- three chronological windows per context, minimum 50 samples each;
- minimum 60 samples in at least two regime slices;
- overall/per-context/window performance floors;
- majority-class and uniform-probability baselines;
- log loss, Brier, 10-bin ECE, chronological stability and class-distribution drift checks.

Possible policy statuses are `insufficient_evidence`, `review_candidate` and `reject`. `review_candidate` means eligible for human consideration only.

Review evidence is deterministically ordered and bound to the exact model/dataset/manual-review record/target, outcome IDs, context boundaries and a versioned evidence fingerprint. Duplicate IDs, duplicate context/bar outcomes and identity mismatch fail closed. Frozen review records are immutable; insufficient evidence cannot be frozen as a review.

Core endpoints:

- `GET /v1/learning/performance-review/status`
- `GET /v1/learning/performance-review/records`
- `POST /v1/learning/performance-review/create`
- `POST /v1/learning/performance-review/report`

Learning-page summary/records now expose this audit state. Backup manifest schema 9 includes performance-review records.

## Safety boundary — unchanged

- MT5 gateway remains read-only; no broker order send/modify/close path exists.
- Legacy Meta remains the live Shadow decision source.
- Learned inference, scoring and performance review remain audit-only.
- `promotion_enabled=false`, `champion_created=false`, `shadow_effect=false`, `risk_effect=false`, `broker_effect=false` in the performance-review path.
- No automatic promotion, live self-modification or Champion activation exists.

## Existing implemented stack

Historical Meta v1/v2, CES and KTE; deterministic saved-history replay; KTE partial-exit ledger and offline trade review; Trend/Regime/Structure/Volatility; Market Opportunities; Gann; broker DOM Depth; Fed/ECB Macro/News context; chronological evaluation; immutable learning snapshots/labels/export; offline baseline trainer; challenger registry; manual review eligibility; read-only challenger inference; exact matured-label scoring; and now challenger performance review.

## Remaining work / next gate

1. Accumulate enough **real** out-of-sample scored observations across contexts/regimes to use the fixed policy meaningfully. Do not retune thresholds post hoc on the same held-out evidence.
2. Run native user-laptop Windows + actual MT5 broker-history/Market Book + Telegram acceptance.
3. Full backup restore, feed-supported order flow/volume profile, portfolio/correlation research and multi-account migration remain later work.
4. Broker execution state machine/reconciliation and any Champion activation are separate future architecture tracks and remain disabled.
5. `main` and `development` are currently unprotected; required status checks are not enforced by GitHub branch protection.

No synthetic/CI/backtest metric is a profitability or future win-rate guarantee.
