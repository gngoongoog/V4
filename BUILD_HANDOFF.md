# BrainTrade OS — promoted 0.6.0 + post-0.6 development handoff

Canonical repository: `gngoongoog/V4`.

## Promoted 0.6.0 baseline

Verified executable/source checkpoint: `fdc63d4d05df89e27607641bd197b9e6ead936e9` (tree `8593783f43a4892cdc724ed5d112607ef7cee006`).

Validation on that exact checkpoint:

- Research CI: PASS — run `38803539711`.
- Windows CI/package: PASS — run `38803539405`.
- Windows artifact: `BrainTrade-Windows-x64-v0.6.0-Preview`, 33,948,849 bytes, SHA-256 `3f7a945a4f9c1ab1ce3e008880722c3dc078a54018a8f622133c604420665779`.
- Windows native-validation artifact: 1,785,325 bytes, SHA-256 `64725943777396d8c974c825a0319554b7f6536213709fd4d76edd49c6a6c66d`.
- Simulated MT5 execution integration: PASS; the harness does not connect to a broker.
- Native broker/account acceptance: NOT PERFORMED.
- Telegram live delivery: NOT PERFORMED.

The reviewed 0.6 line was promoted to `main` through PR #13 without rewriting branch history. Its release evidence remains in `docs/validation.json`, with an immutable archival copy at `docs/validation-v0.6.0.json` on the post-0.6 preparation branch.

## Current post-0.6 development

PR #16 was merged to `development` as merge commit `fd8490e6f33f3bd61e655829c12e7824462694a1`. Its exact tested feature head was `f36b9c1c7bd265675f5802efa8d2eddfc27c1ee8`.

Run `38807773183` passed both Linux and Windows jobs on that exact feature head, including Rust tests, Python contracts, smoke, backup/restore, Studio, simulated execution integration, release packaging and Windows artifact upload.

Feature-build artifacts from that run, before the post-0.6 version bump:

- `BrainTrade-Windows-x64-v0.6.0-Preview`: 34,007,016 bytes; SHA-256 `e4acfc271c0d9a5ba480647f8b5ae907b39692dd700bdb25906c7a9ee9662d96`.
- `windows-native-validation`: 1,785,267 bytes; SHA-256 `6daacbba781834ce9243ab681b3cda3308a3b5dbf3af80346754ea59aaf8eff0`.

Portfolio Exposure v1 is now implemented in `development` and remains OFF by default. When enabled, it applies one aggregate entry budget across all BrainTrade execution identities: maximum combined positions/unresolved opens, combined lots and estimated stop-risk percentage of current equity. Unknown aggregate stop risk fails closed for new entries. Existing positions, closes, broker SL/TP, timed exits and reconciliation are never blocked by this gate. Settings persist in SQLite, survive staged backup/restore and are editable only while STOPPED from the native Execution page.

The Windows preview artifact workflow on `release/0.7-preview` has also been prepared to use a version-agnostic upload name/path so future Cargo version changes do not require another hard-coded workflow rename.

## Operational boundaries

- Startup/restart remains STOPPED.
- Secondary SOLO never arms global START.
- Native MT5 Demo acceptance on the target machine is still pending and must be recorded separately from CI.
- Live Telegram delivery is still pending and must use local credentials that never enter the repository.
- Imported ML challengers remain research-only; there is no automatic Champion promotion or broker activation.
- CI/package/simulated-MT5 evidence does not establish profitability, guaranteed latency or native broker execution quality.

## Next engineering work

1. Reconcile the post-0.6 version line and current-development validation records without relabeling 0.6 evidence as 0.7 evidence.
2. Run the native Windows + Demo MT5 acceptance checklist on the target installation.
3. Test private Telegram delivery with local-only credentials.
4. Enable required checks / branch protection for `main` and preferably `development` when repository administration access is available.
5. Continue out-of-sample evidence, ablation, aligned correlation/portfolio research and historical-event replay while preserving no-lookahead boundaries.

Do not force-update or rewrite `main`/`development` history.
