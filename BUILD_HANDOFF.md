# BrainTrade OS — Build Handoff

## Active checkpoint — 2026-10-14

Canonical repository: `gngoongoog/V4`. Active branch: `development`; `main` remains the older deliberately promoted stable checkpoint. Resume from repository state and current Git heads, never from old ZIPs or chat-only scratch state.

## Executable/code verification boundary

Current code checkpoint: `a74f7ca87497d8e1c923523050acb6bbf293bd7d` (`Guard generated directories in repository contract`).

Research workflow `38544897072`: **PASS** — Rust domain/runtime tests, Core build, Python tests, real-Core smoke, offline browser checks, native Desktop compile, all-page headless render and artifact upload all completed successfully.

Windows workflow `38544897241`: Linux duplicate validation **PASS**. The Windows `Package desktop, core and gateway` job is **IN PROGRESS** at this documentation checkpoint. Do not claim final Windows PASS until GitHub records that job and artifact upload as successful.

Commits after `a74f7ca...` are documentation/reconciliation only unless a newer code checkpoint is explicitly recorded here.

## Non-negotiable safety boundary

- MT5 gateway is read-only: no broker order-send/modify/close path is implemented.
- Live Shadow entry decisions still come from deterministic legacy Meta.
- Trend/Regime/Structure/Volatility, Market Opportunities, Gann, DOM, Macro/News, Meta v2 and learned challenger outputs are observational/audit-only live.
- No Champion model, promotion endpoint, automatic promotion or live self-modification exists.
- Prediction scoring cannot alter Analysis.action, Meta, opportunities, risk, lot sizing, Shadow or broker behavior.
- DOM means broker-provided resting Market Book depth only; tick-volume/profile values are proxies, not global exchange order flow.

## Implemented research / analytical stack

- Historical Meta v1, Meta v2, CES and KTE over saved immutable History with deterministic replay and same-data experiments.
- KTE TP1/TP2/TP3 partial ledger, break-even, ATR trailing, time limits, cooldown and conservative OHLC ambiguity handling; one saved ledger drives detailed report/CSV/JSON/HTML outputs.
- Trend v1, Regime v1, Structure v1, Volatility v1.
- Market Opportunities: structural zones/support/resistance/swings, multi-timeframe momentum/divergence, tick-activity/profile proxies, sessions, setup detection and current-quote Entry/SL/TP1/TP2/TP3 projection with stale-value withdrawal.
- Gann Geometry v1 observational context.
- Broker-supported DOM Depth observational context.
- Fed/ECB Macro/News Context observational risk context.
- Chronological evaluation over immutable saved histories with causal holdout/rolling windows, persisted source/evidence identity and native Desktop review.

## Learning pipeline — implemented

1. `learning_snapshots` stores immutable completed-bar features using `gold-m1-feature-v1`.
2. `learning_labels` matures exact 5/15/30-minute outcomes using `gold-m1-outcome-v1`; gaps are never shifted or fabricated.
3. Deterministic dataset export feeds offline `scripts/train_linear_baseline.py` with chronological splits, 30-row purge and train-only normalization.
4. `/v1/learning/register` stores immutable `unreviewed_challenger` artifacts; no Champion is created.
5. `manual-review-eligibility-v1` requires compatible persisted chronological evidence for every training context before an immutable review record may be created.
6. Read-only challenger inference consumes the exact stored feature payload; Rust/Python parity fixtures guard feature ordering/normalization/class probabilities.
7. `learning_predictions` stores immutable predictions tied to model/dataset/review/context/bar identity.
8. `prediction-outcome-scoring-v1` waits for the exact matured target horizon and stores immutable realized outcomes.
9. Scoring aggregates sample/class support, confusion, accuracy, balanced accuracy, macro-F1, log-loss, Brier, confidence and 10-bin ECE. Current sample floors are reporting warnings only; `promotion_eligible=false`.
10. Backup schema 8 includes current research/chronological/learning registry/review/prediction/scoring audit state; full restore remains unimplemented.

## Quality audit / repository hardening

- SQLite explicitly enables foreign keys and runs startup `foreign_key_check`; regression coverage rejects orphan rows.
- Project contract tests fail on broker-write APIs, unsafe model promotion/execution coupling and forbidden tracked runtime/generated artifacts.
- Windows workflow now runs on `development`.
- Source-only ZIP and portable `Source/` copy exclude secrets, runtime DB/log state, `.import`, old archives, `.ci`, dependency/cache/venv/node_modules trees while preserving normal source such as `crates/runtime`.
- Source ZIP freezes its source file list before creating the temporary archive, preventing self-inclusion for an output placed inside the source tree.
- Windows dependency metadata is resolved for the MSVC target actually used by CI.
- GitHub branch protection remains disabled; use expected-SHA/non-force writes, CI and PR/checkpoint discipline.

## Current documentation reconciliation

`README.md`, `PROJECT_STATE.md`, `ROADMAP.md` and `docs/QUALITY_AUDIT.md` are already reconciled. This checkpoint also reconciles `CONTINUE_HERE.md`, this handoff, `docs/LEARNING.md`, `docs/RESEARCH.md`, `docs/ARCHITECTURE.md`, `docs/RELEASE_STATUS.md`, `docs/validation.json`, `docs/BRANCH_POLICY.md` and `CHANGELOG.md`.

## Exact next action

1. Re-check Windows run `38544897241` until the Windows package/artifact job reaches a terminal state.
2. If it fails, inspect that exact job and fix packaging without weakening source/security/causality guards.
3. If it passes, update only the final Windows status fields in this handoff / validation / release-quality docs and consider a deliberate PR promotion of the safe development checkpoint to `main`.
4. Then begin the **Challenger Performance Review Policy** as a separate read-only scientific layer over immutable scored outcomes.

That next policy must require sufficiently large samples and class support, multiple chronological/context windows, transparent baselines, calibration/stability/drift checks and immutable evidence linkage. It may output `insufficient_evidence`, `review_candidate` or `reject`; it must not create a Champion, auto-promote or influence Shadow/risk/broker execution.

Never commit credentials, private runtime DB/logs, `.import`, encoded chunks, generated dependency trees or ZIP-only source.
