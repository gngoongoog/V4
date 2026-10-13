# BrainTrade OS — Build Handoff

This is the first file to read when resuming work after a chat/session interruption.

## Canonical repository

- Repository: `gngoongoog/V4`
- Stable branch: `main`
- Ongoing work branch: `development`
- Current development checkpoint: **0.4.0 in progress**
- Last validated source baseline: **0.3.0**
- Last delivered Windows portable: **0.2.0 Preview**

## Last safe checkpoint

`main` contains the migrated BrainTrade source as normal GitHub files/folders, a normally generated `Cargo.lock`, and the permanent `.github/workflows/windows.yml`. Legacy `.import` staging/chunk files and one-time migration workflows are not part of the working tree.

`development` is ahead of `main` with the first KTE validation batch. It adds deterministic trade-management regression coverage in `crates/domain/src/backtest/kte.rs` and keeps the implementation itself on the same stateful KTE execution path.

## Current active work

Finish the 0.4.0 KTE checkpoint before starting later analysis brains.

The current KTE validation batch covers:

1. deterministic 40/30/30 lot splitting and rejection when three valid partials cannot be formed;
2. TP1 partial exit followed by next-bar break-even activation;
3. TP2 partial exit followed by next-bar ATR trailing activation;
4. TP3 final close and aggregate trade PnL/ledger fields;
5. conservative stop priority when stop and target are both inside the same OHLC range, with ambiguity flagging;
6. 45-minute pre-TP1 and 90-minute post-TP1 time limits;
7. exactly three following bars of cooldown after a complete KTE exit.

## Changed files/modules in current WIP

- `crates/domain/src/backtest/kte.rs` — KTE regression tests plus an explicit `cooldown_active` helper used by the execution path.

## Verification

- Repository migration to normal files: **PASS** by GitHub tree inspection.
- `.import` / encoded staging in working tree: **ABSENT** by GitHub tree inspection.
- KTE regression batch: **CI PENDING / not yet promoted to main**. A draft pull request from `development` to `main` was opened so the permanent GitHub Actions workflow can validate the exact branch state.
- Prior Python gateway/packaging suite: **16/16 PASS** on the earlier 2026-10-13 build checkpoint; this is prior validation, not a substitute for fresh branch CI.
- Prior 0.3.0 Rust baseline: 18 domain + 13 runtime tests passed; again, prior validation only.

## Known incomplete work

- Do not call 0.4.0 fully validated until the current CI run completes successfully and any failures are fixed.
- KTE chart/report/export consistency with the saved multi-fill ledger still needs direct review/regression coverage.
- Validation/release documentation still needs reconciliation after KTE tests are green.
- Native Windows/MT5 historical validation remains a later laptop/broker checkpoint.

## Resume procedure

1. Read this file, `PROJECT_STATE.md`, `ROADMAP.md`, `CHANGELOG.md` and `docs/CONTINUITY_PROTOCOL.md`.
2. Inspect `development` and the draft KTE pull request before editing anything.
3. Check the newest GitHub Actions result for the `development` head.
4. If CI fails, fix the exact failing test/build on `development` and update this file.
5. If CI passes, continue with KTE chart/report/export ledger consistency, then reconcile validation docs.
6. Promote only a safe/resumable tested checkpoint to `main`.

## Rules for future ChatGPT sessions

- GitHub is the source of truth when chat memory disagrees.
- Do not rebuild existing repository files from memory.
- Do not overwrite newer repository changes without comparing first.
- Do not mark unfinished features as complete.
- Do not commit credentials or secret local configuration.
- Do not reintroduce encoded staging, chunk files, `.import`, or ZIP-only source storage.

## Next exact action

Inspect the latest CI result for the draft KTE pull request. Fix failures if any. When green, verify that HTML/CSV/JSON KTE exports are derived from the same saved `Trade.fills`, `Trade.stop_updates` and `Trade.targets` ledger used by the backtest result, add regression coverage for that invariant, then update validation artifacts.
