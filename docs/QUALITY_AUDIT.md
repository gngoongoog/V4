# BrainTrade OS — Quality Audit

Audit date: 2026-10-14. Canonical repository: `gngoongoog/V4`.

This document records the repository-wide consistency review across the Rust workspace, native desktop, read-only MT5 gateway, historical research, learning pipeline, persistence, packaging, CI and documentation. It is not a profitability or live-trading approval.

## Architecture and data-flow checks

- `crates/domain` owns deterministic research/backtest/opportunity contracts.
- `crates/runtime` owns persistence, Core IPC, Shadow, historical services, chronological evaluation, learning registry/review/inference/scoring and backup metadata.
- `crates/desktop` consumes Core APIs and renders native Slint pages; it does not own broker execution.
- `gateway/mt5_gateway.py` remains read-only. Repository contract tests reject broker-write APIs such as `order_send` and `TRADE_ACTION_*`.
- The learning live path is ordered as completed-bar observation -> read-only challenger inference -> matured-label scoring -> publish status. Learned outputs do not feed legacy Meta, opportunities, risk, lot sizing, Shadow execution or MT5 orders.

## Persistence integrity

SQLite startup explicitly enables `PRAGMA foreign_keys=ON` and runs `foreign_key_check`. A unit test verifies orphan inserts are rejected. WAL/FULL synchronous behavior remains in place. Backup manifest schema 8 includes settings/audit, historical data, chronological evidence, learning registry/review, read-only predictions and scored outcomes. Full restore is still intentionally not implemented.

## Source/package hygiene

Both `scripts/source_archive.py` and portable `source_copy()` defensively exclude runtime state, `.import`, archives, secrets, generated dependency trees and caches while preserving real source such as `crates/runtime` and `.env.example`. Tests cover output-inside-source-tree behavior and exclusions for `.ci`, `node_modules`, virtual environments, Python caches, SQLite/log/key/certificate files and ZIP/7z/RAR artifacts. `test_project_contract.py` also fails CI if these artifacts become tracked later.

## Validation boundary — PASS

Executable/code checkpoint: `a74f7ca87497d8e1c923523050acb6bbf293bd7d`.

- Research validation run `38544897072`: **PASS** including Rust/Core/Python, real-Core smoke, browser checks, Desktop compile/all-page headless render and artifact upload.
- Windows build/tests run `38544897241`: **PASS** including Linux duplicate validation, Windows portable packaging and artifact upload.
- Windows artifact: `BrainTrade-Windows-x64-v0.3.0-Preview`, 32,991,252 bytes, SHA-256 `b417c82c5f105624292bef10061f9254cfb0a6949aed899ad116e30a85e04a76`.

Documentation-only commits after that checkpoint do not change executable behavior. Native acceptance on the user's own Windows/MT5 broker environment remains separate from CI.

## Findings fixed during this audit

- Windows CI now runs on `development`, not only `main`/PR.
- Source packaging exclusions were hardened and aligned between both export paths.
- SQLite foreign-key enforcement became explicit and tested.
- Permanent project-contract tests now guard read-only broker access, no promotion/execution routes, causal learning order, shared schemas, CI coverage and repository hygiene.
- Project documentation was reconciled with already implemented Gann/DOM opportunities, chronological evaluation, manual review eligibility, read-only inference and prediction outcome scoring.

## Open limitations / not defects

- `main` and `development` are currently unprotected GitHub branches; required status checks are not enforced by branch protection. Until repository settings are changed, discipline depends on the documented branch policy and CI review.
- `development` is intentionally ahead of `main`; promotion to `main` should happen only after a deliberate safe-checkpoint review.
- Version normalization is implemented in the 0.4.0 candidate; Cargo regenerated the three workspace lockfile versions without third-party upgrades.
- Native Windows + the user's real MT5 broker history, live Telegram delivery, full restore, broker execution, portfolio execution risk and autonomous champion promotion are not validated/implemented.
- Historical/synthetic metrics are implementation evidence, not proof of profitability or future win rate.

## Additional 0.4.0 release review

- Compared both repositories; V4 includes all other-repository paths and newer analytical/learning layers.
- Added 13-page native coverage, with explicit Opportunities, Market Map and Learning surfaces.
- STOP and stale quotes/market snapshots withdraw both specialist and legacy Meta plans; stale news items are withdrawn on Core loss.
- Opportunity caches compare completed-bar inputs on every timeframe and react to broker corrections.
- Opportunity history/export exposes immutable first-observed records, explicitly marked historical.
- DOM subscription failures retry with backoff; bounded books preserve both sides, aggregate duplicate price levels and exclude market-order rows from resting-depth analysis.
- Packaged Windows executable now undergoes real Core startup, DPAPI IPC, config save, backup, shutdown and headless page rendering.
- Fresh validation for these changes is tracked in BUILD_HANDOFF.md; the earlier PASS block above belongs to its older commit.

## Next quality gate

The next scientific layer is an explicit Challenger Performance Review Policy over sufficiently large immutable scored out-of-sample samples. It may produce a manual review recommendation only; it must not auto-promote a Champion or influence Shadow/broker execution.
