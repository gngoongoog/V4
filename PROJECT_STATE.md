# BrainTrade OS — Project State

Canonical repository: `gngoongoog/V4`. Stable/resumable branch: `main`. Active research branch: `development`.

Current release candidate: **0.4.0 Preview**. Cargo, lockfile, desktop and Windows package versions are aligned. The user has requested Windows delivery; use BUILD_HANDOFF.md and docs/validation.json for the exact final validation boundary.

## Implemented

### Core / desktop / MT5 boundary

- Independent Rust Core with authenticated loopback IPC and native Slint desktop.
- Start, Stop, reconnect and full-exit lifecycle controls. STOP/stale data immediately withdraw quote plans.
- 13 native pages including Opportunities/history, Market Map and Learning/model review.
- Read-only MT5 account/positions/orders/history/quotes/Market Book access.
- Persistent rule-based Shadow simulation and deterministic risk limits.
- Explicit repository contract tests reject broker-write APIs and learned-model execution/promotion routes.

### Historical research

- Meta v1, Meta v2 candidate, CES v1 and KTE v1 historical engines.
- Frozen input/options, exact saved History, offline replay and same-data experiments.
- KTE three-leg fills, TP1 break-even, TP2 ATR trailing, TP3 final close, 45/90-minute limits, cooldown and conservative stop-first ambiguity.
- Saved ledger drives report JSON/text, trades/fills/stops/targets/equity CSV and self-contained HTML trade review.
- Persisted chronological Meta v1/v2 holdout/rolling evaluation over immutable saved backtest sources.

### Live observational analysis

- Trend, Regime, Structure, Volatility and Meta v2 candidate.
- Market Opportunities with structural zones/setups, Gann geometry and broker-supported DOM Depth.
- Macro/News context from official Fed/ECB RSS freshness state.
- These newer brains remain observational. Legacy Meta still controls Shadow entries.

### Learning Foundation

- Immutable `gold-m1-feature-v1` completed-bar snapshots.
- Exact delayed `gold-m1-outcome-v1` labels at 5/15/30 minutes; missing/gapped horizons are not shifted or fabricated.
- Deterministic dataset export and offline class-weighted linear-softmax baseline trainer.
- Immutable challenger registry with dataset identity and chronological split metadata.
- Persisted chronological evaluation evidence and manual review-eligibility records.
- Read-only challenger inference using the exact stored learning feature payload.
- Immutable matured prediction outcomes and descriptive scoring by model/target: accuracy, balanced accuracy, macro-F1, log loss, Brier score, confidence/calibration and class/confusion support.
- No champion promotion, Shadow effect, broker effect or live self-modification.

### Persistence / packaging / CI

- SQLite WAL + synchronous FULL, explicit `foreign_keys=ON`, startup `foreign_key_check`, and a unit test that rejects orphan foreign-key inserts.
- Backup schema 8 includes historical/chronological/learning registry/review/prediction/scoring audit state; secrets are excluded. Full restore is not implemented.
- Both source export paths exclude runtime state, secrets, archives, generated dependency trees/caches and `.import`; tests preserve real source such as `crates/runtime`.
- Research CI and Windows CI run on `development`; Windows CI includes the packaged Core DPAPI/startup/config/backup/shutdown checks and all-page rendering; it does not substitute for native broker acceptance.

## Validation boundary

The latest executable checkpoint being validated is recorded in `BUILD_HANDOFF.md`, with run IDs and final PASS/FAIL state mirrored in `docs/validation.json`. Documentation-only reconciliation after that checkpoint does not alter executable behavior.

## Known limitations / pending

- Native Windows + real MT5 broker-history acceptance and live Telegram delivery with local credentials.
- Real-broker-data scientific evaluation large enough to judge candidate effectiveness.
- Explicit challenger performance-review policy over sufficiently large scored out-of-sample samples.
- True feed-supported order flow / volume profile beyond broker Market Book and candle tick-volume proxies.
- Portfolio/correlation execution risk, full backup restore and multi-account migration.
- Broker order state machine, reconciliation and live/demo execution.
- Automated champion promotion remains intentionally absent.
- `main` and `development` currently have no GitHub branch protection / required-status enforcement.

No profitability, win-rate or broker-latency guarantee is established by synthetic fixtures, CI, backtests or model metrics.

## Continuity

GitHub normal files/folders are the source of truth. Never reconstruct over newer code from an old ZIP. Never commit credentials, tokens, private runtime databases/logs, generated dependency trees, `.import`, encoded chunks or ZIP-only source.
