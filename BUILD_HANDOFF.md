# BrainTrade OS — Build Handoff

This is the first file to read when resuming work after a chat/session interruption.

## Canonical repository

- Repository: `gngoongoog/V4`
- Stable branch: `main`
- Ongoing work branch: `development`
- Current development checkpoint: **0.4.0 in progress**
- Last delivered Windows portable: **0.2.0 Preview**

## Repository state

`main` is the safe pre-KTE-validation checkpoint. `development` contains the active 0.4.0 KTE validation work. Source is stored as normal GitHub files/folders. Legacy `.import`, encoded staging, chunk transport and ZIP-only source storage are not part of the working tree. `Cargo.lock` is generated normally by Cargo.

## 0.4.0 work now present on development

KTE historical execution and regression coverage include:

1. deterministic 40/30/remainder lot splitting and rejection when three valid partials cannot be formed;
2. TP1 partial exit followed by next-minute break-even activation;
3. TP2 partial exit followed by next-minute ATR trailing activation;
4. TP3 final close and aggregate trade accounting;
5. conservative stop priority when stop and target are both inside the same unknown OHLC range, with ambiguity persisted;
6. 45-minute pre-TP1 and 90-minute post-TP1 time limits;
7. exactly three following bars of cooldown after complete exit.

The reporting path was also reconciled. `Trade.fills`, `Trade.stop_updates` and `Trade.targets` are now the common saved ledger used by the desktop saved-report text and detailed export helpers. Export adds `fills.csv`, `stop_updates.csv` and `targets.csv` while retaining `trades.csv`. HTML includes a readable ledger section and fill markers on the equity SVG derived from the same fill records. The real-core integration harness verifies detailed export files exist and an unchanged offline replay reproduces them byte-for-byte.

The prior Windows packaging failure was isolated to `os.fsync()` on a temporary ZIP opened read-only. `scripts/package-portable.py` now opens the temporary archive with `rb+`, flushes it and then calls `fsync`, preserving the durability check instead of deleting it.

## Fresh verification

Latest completed Linux CI on the active 0.4.0 path:

- braintrade-domain: **25 passed / 0 failed**.
- braintrade-runtime: **14 passed / 0 failed / 1 explicit live-network test ignored**.
- Python gateway/packaging suite: **16 passed**.
- `braintrade-core` build: **PASS**.
- real-core smoke harness: **PASS**, including detailed export files and deterministic offline replay.

Current Windows packaging CI: **PENDING at this handoff snapshot**. Do not mark it PASS until GitHub reports completion successfully. A CI-produced Windows ZIP is still a validation artifact; native laptop/MT5 acceptance is a later step.

## Changed areas in this checkpoint

- `crates/domain/src/backtest/kte.rs` — KTE regression tests and cooldown helper.
- `crates/runtime/src/backtest.rs` — common KTE ledger reporting, detailed CSV exports, HTML fill markers and ledger consistency regression test.
- `scripts/package-portable.py` — Windows-compatible temporary ZIP `fsync` handling.
- `scripts/check_backtest.py` — end-to-end detailed ledger export and replay checks.
- `docs/BACKTEST.md`, `docs/validation.json`, `docs/RELEASE_STATUS.md`, `PROJECT_STATE.md`, `ROADMAP.md`, `CHANGELOG.md` — 0.4.0 state reconciliation when committed.

## Known incomplete work

- Final Windows CI result for the latest development head.
- Native Windows/MT5 historical validation against the user's actual broker/account.
- Live Telegram delivery with local credentials.
- Broker execution/protection and the later analysis/learning brains remain roadmap work.

## Resume procedure

1. Read this file, `PROJECT_STATE.md`, `ROADMAP.md`, `CHANGELOG.md` and `docs/CONTINUITY_PROTOCOL.md`.
2. Inspect `development` and draft PR #1 before editing.
3. Check the newest GitHub Actions run for the current development head.
4. If Windows CI failed, inspect the exact failing job log and fix only that failure on `development`.
5. If both Linux and Windows jobs are green, update any pending validation wording, then promote the tested resumable checkpoint to `main`.
6. After 0.4.0 source promotion, proceed to P1 laptop/MT5 acceptance or P2 analysis expansion according to the user's instruction.

## Rules for future ChatGPT sessions

- GitHub is the source of truth when chat memory disagrees.
- Do not rebuild existing repository files from memory.
- Do not overwrite newer repository changes without comparing first.
- Do not mark unfinished features or tests as complete.
- Do not commit credentials or secret local configuration.
- Do not reintroduce encoded staging, chunk files, `.import`, or ZIP-only source storage.

## Next exact action

Check the latest Windows CI job for `development`. If green, finalize the validation wording and promote the 0.4.0 source checkpoint to `main`. If it fails, fix the exact packaging/build error, rerun CI, and keep `main` unchanged until green.
