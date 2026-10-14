# 0.4.0 portable release candidate — 2026-10-14

- Compared both repositories and retained V4 as the fuller source.
- Added Opportunities, Market Map and Learning pages, with all existing learning actions available from the dashboard.
- Added first-observed opportunity history and export; historical records are explicitly not current quotes.
- STOP immediately withdraws quote plans; stale maps cannot present old levels as current.
- Opportunity cache reacts to corrected history on any timeframe.
- Normalized Cargo/UI/portable version to 0.4.0; added source commit to package metadata.
- Added packaged Windows startup/DPAPI/config/backup/shutdown and native-render verification.
- Validation pending for this candidate; earlier results below retain their original boundaries.

# Changelog

## 2026-10-14 — learning audit pipeline and repository quality reconciliation

- Completed persisted chronological Meta evaluation over immutable saved History.
- Added manual challenger review-eligibility records bound to immutable model/dataset/context evidence.
- Added read-only challenger inference from the exact stored 87-feature learning payload with Rust/Python parity guards.
- Added exact matured-label prediction outcome scoring with accuracy, balanced accuracy, macro-F1, log loss, Brier, confidence/calibration and class/confusion support.
- Kept learned outputs disconnected from Legacy Meta, Shadow, risk, lot sizing and broker execution; champion promotion/activation remains disabled.
- Enabled explicit SQLite foreign keys plus startup integrity checking and orphan-FK regression coverage.
- Hardened both source export paths against secrets, runtime state, archives, `.import`, dependency trees, virtual environments and generated caches.
- Expanded permanent project-contract tests for repository hygiene, read-only MT5 access, learning causal order, shared schemas and absence of learned execution/promotion routes.
- Windows CI now validates `development` in addition to `main`/PR.
- Reconciled README/state/roadmap/architecture/research/learning/acceptance/continuity documentation with implemented code.
- Added `docs/QUALITY_AUDIT.md`; native user-laptop MT5/Telegram acceptance and full restore remain pending.

## 2026-10-14 — selectable historical Meta v2 and matched comparisons

- Added versioned Meta v2 historical candidate with the same exit/cost/risk model as Meta v1.
- Persisted candidate gates and engine admission outcomes for every tested minute.
- Added native strategy selection, CSV/JSON audit, HTML gate review and side-by-side metrics.
- Saved experiment ancestry atomically; verified exact history/options equality before comparisons and preserve it on offline replay.
- Added prefix/gap/risk fixtures, pre-change legacy report digests, persistence/replay integration and browser checks.
- Live Shadow remains legacy Meta; broker execution remains disabled.

## 2026-10-13 — offline candlestick review on development

- Added per-trade M1 candles from saved history, entry/target/fill markers and effective stop segments to the self-contained HTML export.
- Added trade selection, UTC times, gap boundaries, costs, partial-fill detail, legacy Meta/CES fallback and compact no-trades state.
- Added a real-Core three-target KTE fixture, identical offline HTML/CSV/JSON replay and browser interaction checks.
- Reconciled continuation docs with observational Trend/Regime, Structure/Volatility and Meta v2 checkpoints.

## 0.4.0 — research milestone in progress

- Added KTE signal and stateful historical execution with three-leg partial exits.
- Added TP1/TP2/TP3 handling, next-bar break-even/trailing, 45/90-minute time limits and three-following-bar cooldown.
- Added conservative stop-first handling for ambiguous OHLC stop/target overlap.
- Unified detailed fills/stop updates/targets behind the saved Trade ledger and deterministic exports/replay.
- Added Market Opportunities and observational Gann/DOM/Macro-News research layers.
- Added chronological evaluation and Learning Foundation research infrastructure.
- Durable source remains normal Git files/folders; encoded staging/chunk transport is not part of the working tree.

## 0.3.0 — source/package metadata baseline

- Historical Meta/CES lab and asynchronous MT5 history requests.
- Persistent reports/raw inputs and deterministic offline replay.
- Same-data experiments and CSV/JSON/HTML exports with equity curve.
- Official Fed/ECB RSS page/cache and Telegram read-only handlers.
- Cargo and current Windows preview package metadata still use 0.3.0 during the 0.4.0 research milestone.

## 0.2.0 — last declared delivered Windows portable preview

- Last end-user portable declared before the current research-source milestone.
