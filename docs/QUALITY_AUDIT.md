# BrainTrade OS — Quality Audit

Audit date: 2026-10-14. Canonical repository: `gngoongoog/V4`.

This audit covers the Rust workspace, native desktop, read-only MT5 gateway, historical research, learning pipeline, persistence, packaging, CI and documentation. It is not a profitability or live-trading approval.

## Architecture / safety

- `crates/domain` owns deterministic research/backtest/opportunity contracts.
- `crates/runtime` owns persistence, Core IPC, Shadow, historical services, chronological evaluation and learning review/inference/scoring/performance-review audit services.
- `crates/desktop` consumes Core APIs and renders native Slint pages; it does not own broker execution.
- `gateway/mt5_gateway.py` remains read-only; repository contracts reject broker-write APIs.
- Learning live order is completed-bar observation -> read-only challenger inference -> matured-label scoring -> performance-review audit/status. None of these learned outputs feed Legacy Meta, opportunities, risk, lot sizing, Shadow execution or MT5 orders.

## Persistence / package integrity

- SQLite uses WAL/FULL synchronous behavior, explicitly enables foreign keys and runs startup `foreign_key_check`.
- Backup manifest schema 9 includes settings/audit, historical data, chronological evidence, learning registry/review, predictions, scored outcomes and performance-review records. Full restore is intentionally not implemented.
- Source/package paths exclude secrets, runtime DB/log state, `.import`, archives and generated dependency/cache/venv trees.

## Current validation boundary — PASS

Executable/code checkpoint: `74e6387e3f66b8035f5ce55db92561ec1809ebf4`.

- Research run `38586058214`: **PASS** including Rust/Core/Python, real-Core smoke, browser checks, Desktop compile/all-page render and artifact upload.
- Windows run `38586058215`: **PASS** including Linux duplicate validation, Windows workspace/Python tests, release portable packaging, packaged runtime/native checks and artifact uploads.
- Windows artifact: `BrainTrade-Windows-x64-v0.4.0-Preview`, 33,229,148 bytes, SHA-256 `e9e09749359a8bfd4cc14959d141d1c8485bf3fe427887413f4b2c5274db0237`.

Documentation-only commits after this checkpoint do not change executable behavior. Native acceptance on the user's own Windows/MT5 broker environment remains separate.

## Challenger performance-review audit result

`challenger-performance-review-v1` is implemented as an observational/manual policy over immutable scored outcomes. It uses fixed sample/class/context/window/regime gates, majority/uniform baselines, balanced accuracy/macro-F1/log-loss/Brier/ECE, chronological stability and distribution-drift checks. Exact evidence identities, outcome IDs, boundaries and reason codes are persisted. A favorable result is only `review_candidate` for human consideration. Champion creation, promotion, Shadow/risk effect and broker effect remain disabled.

## Open limitations / not defects

- `main` and `development` are unprotected; required status checks are not enforced by branch protection.
- Native Windows + actual MT5 broker history/Market Book and live Telegram delivery have not been accepted on the user's machine.
- Real out-of-sample evidence is not yet large enough by definition to claim candidate effectiveness simply because the policy exists.
- Full restore, true feed-supported order flow/volume profile, portfolio execution risk, broker execution/reconciliation and autonomous champion promotion are not implemented.
- Historical/synthetic metrics are implementation evidence, not proof of future profitability or win rate.

## Next quality gate

Accumulate fresh real out-of-sample scored evidence under the fixed review policy and run the native user-laptop MT5/broker + Telegram acceptance checklist. Do not turn a favorable research review into automatic trading authority.
