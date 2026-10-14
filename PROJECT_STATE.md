# BrainTrade OS — Project State

Canonical repository: `gngoongoog/V4`. Stable/resumable branch: `main`. Active research branch: `development`.

Current release candidate: **0.4.0 Preview**. Exact tested executable boundary is in `BUILD_HANDOFF.md` and `docs/validation.json`.

## Implemented

### Core / desktop / MT5 boundary

- Independent Rust Core with authenticated loopback IPC and native Slint desktop.
- 14 native pages including Opportunities/history, Market Map and Learning/model review and Backup/Restore.
- Read-only MT5 account/positions/orders/history/quotes/Market Book access.
- Persistent deterministic Shadow simulation and risk limits; Legacy Meta remains the Shadow entry source.
- STOP/stale-data handling withdraws new quote plans; repository guards reject broker-write APIs.

### Historical / observational research

- Historical Meta v1, Meta v2 candidate, CES v1 and KTE v1 with frozen saved inputs, deterministic replay and same-data experiments.
- KTE multi-leg ledger, break-even/trailing/time limits/cooldown and offline candlestick trade review.
- Trend, Regime, Structure, Volatility, Market Opportunities, Gann Geometry, broker DOM Depth and Fed/ECB Macro/News context.
- Persisted chronological evaluation over immutable saved histories.

### Learning pipeline

- Immutable completed-bar feature snapshots and exact delayed 5/15/30-minute outcome labels.
- Deterministic dataset export and offline linear-softmax baseline trainer.
- Immutable challenger registry with dataset/split identity.
- Persisted chronological evidence and manual review-eligibility records.
- Read-only challenger inference from the exact stored feature payload.
- Immutable matured prediction outcomes with accuracy, balanced accuracy, macro-F1, log loss, Brier, confidence/ECE and class/confusion metrics.
- `challenger-performance-review-v1`: fixed sample/class/context/window/regime evidence gates, transparent baselines, calibration/stability/drift checks, immutable reason-coded review records and native Learning-page status.
- Performance review may return `insufficient_evidence`, `review_candidate` or `reject`; it never creates a Champion or affects Shadow/risk/broker behavior.

### Persistence / packaging / CI

- SQLite WAL + synchronous FULL, foreign keys enabled and startup integrity checked.
- Backup manifest schema 10 includes historical/chronological/learning registry/review/prediction/scoring/performance-review audit state. Validated staged restart restore, rollback snapshots and native controls are implemented.
- Source/package hygiene excludes secrets, runtime DB/log state, archives and generated dependency/cache trees.
- Validated executable checkpoint `42c8c6dfda42acfa02de733f3acb93ff639075cc` passed Research workflow `38601402760` and Windows workflow `38601402734`.
- Original GitHub artifact (outer ZIP): `BrainTrade-Windows-x64-v0.4.0-Preview`.

## Known limitations / pending

- Native acceptance on the user's actual Windows laptop, MT5 broker history/Market Book and live Telegram credentials.
- Enough real out-of-sample observations to judge challenger behavior with the fixed review policy.
- True feed-supported order flow/volume profile beyond broker Market Book and candle tick-volume proxies.
- Portfolio/correlation execution risk and multi-account migration.
- Broker order state machine, reconciliation and Demo/Live execution.
- Champion activation/automatic promotion remains intentionally absent.
- `main` and `development` have no required-status branch protection.

No profitability, win-rate or broker-latency guarantee is established by fixtures, CI, backtests or model metrics.

## Continuity

GitHub normal files/folders are the source of truth. Never reconstruct over newer code from an old ZIP and never commit credentials/private runtime state/generated dependency trees.
