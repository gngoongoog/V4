# 0.5.0 extension audit

Trading modes preserve open-position geometry and costs across mode changes/restarts. Sniper checks quote timestamp, cooldown and cost fraction. Higher-frame reads validate account/contract/closure and never synthesize unavailable history. Strategy jobs freeze costs at queue time, use purged chronological splits and refuse exposed test reuse. Wyckoff observations mature only after future bars; duplicate signals remain immutable. New pages are included in 34-frame native acceptance. Exact current validation is in validation.json. Code `a5961ff85b41e50e18a7c8295244d115f9fcfd3d` passed Research `38689840746` and Windows `38689840745`, including all 34 complete native frames.

The following audit records the previous 0.4.0 release and is retained as historical evidence only.

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
- Backup manifest schema 10 includes settings/audit, historical data, chronological evidence, learning registry/review, predictions, scored outcomes and performance-review records. Validated staged restart restore, rollback snapshots and native controls are implemented.
- Source/package paths exclude secrets, runtime DB/log state, `.import`, archives and generated dependency/cache/venv trees.

## Current validation boundary — PASS

Executable/code checkpoint: `42c8c6dfda42acfa02de733f3acb93ff639075cc`.

- Research run `38601402760`: **PASS** including Rust/Core/Python, real-Core smoke, browser checks, Desktop compile/all-page render and artifact upload.
- Windows run `38601402734`: **PASS** including Linux duplicate validation, Windows workspace/Python tests, release portable packaging, packaged runtime/native checks and artifact uploads.
- Original GitHub artifact (outer ZIP): `BrainTrade-Windows-x64-v0.4.0-Preview`, 33,345,482 bytes, SHA-256 `2d8115e64c91cf39231577a2f662bf80430646af61c0579aa108ffbd1fd84585`.

Documentation-only commits after this checkpoint do not change executable behavior. Native acceptance on the user's own Windows/MT5 broker environment remains separate.

## Challenger performance-review audit result

`challenger-performance-review-v1` is implemented as an observational/manual policy over immutable scored outcomes. It uses fixed sample/class/context/window/regime gates, majority/uniform baselines, balanced accuracy/macro-F1/log-loss/Brier/ECE, chronological stability and distribution-drift checks. Exact evidence identities, outcome IDs, boundaries and reason codes are persisted. A favorable result is only `review_candidate` for human consideration. Champion creation, promotion, Shadow/risk effect and broker effect remain disabled.

## Open limitations / not defects

- `main` and `development` are unprotected; required status checks are not enforced by branch protection.
- Native Windows + actual MT5 broker history/Market Book and live Telegram delivery have not been accepted on the user's machine.
- Real out-of-sample evidence is not yet large enough by definition to claim candidate effectiveness simply because the policy exists.
- True feed-supported order flow/volume profile, portfolio execution risk, broker execution/reconciliation and autonomous champion promotion are not implemented.
- Historical/synthetic metrics are implementation evidence, not proof of future profitability or win rate.

## Next quality gate

Accumulate fresh real out-of-sample scored evidence under the fixed review policy and run the native user-laptop MT5/broker + Telegram acceptance checklist. Do not turn a favorable research review into automatic trading authority.

## Final integration checks

Sealed backup validation is read-only and rejects unexpected sidecars without deleting them. Native restore requires STOP, creates rollback state and exposes the last outcome. Performance-review creation binds the displayed model/evidence; the v2 fingerprint includes regime slices.

An earlier Windows render invocation returned before all frames were produced. The final harness waits and requires all 28 exact-size pixel payloads; this corrected check passed.
