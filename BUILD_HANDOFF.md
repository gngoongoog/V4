# BrainTrade OS — Build Handoff

Canonical repository: `gngoongoog/V4`. Active branch: `development`. Release: **0.4.0 Preview**.

## Validated executable boundary

Code checkpoint: `42c8c6dfda42acfa02de733f3acb93ff639075cc`.
Research workflow `38601402760`: **PASS**. Windows workflow `38601402734`: **PASS**.

- 49 domain + 39 runtime Rust tests passed; one live-network test intentionally ignored.
- 32 Python tests passed on Linux and Windows.
- Real-Core lifecycle/auth/staleness/risk, historical/replay/chronological/learning integration passed.
- Corrupt/traversal backup rejection, staged restore/restart and rollback-copy checks passed.
- Offline candlestick report interactions passed.
- All 14 native pages rendered at 1120×820 and 880×700; Windows validates all 28 complete pixel payloads before reporting success.
- Packaged Windows EXE startup, DPAPI IPC, stopped/read-only defaults, config/backup, staged restore/restart and IPC-key preservation passed.

Original GitHub Actions outer artifact: `BrainTrade-Windows-x64-v0.4.0-Preview`, ID `11915201695`, 33,345,482 bytes, SHA-256 `2d8115e64c91cf39231577a2f662bf80430646af61c0579aa108ffbd1fd84585`.
The final portable is a documentation-only repack with a separate delivery digest. Its tested EXE/runtime bytes remain unchanged; DELIVERY_VERIFICATION.json distinguishes executable and documentation commits.

## Completed integration

V4 contains the other repository's paths plus the fuller research stack; see docs/REPOSITORY_COMPARISON.md. Historical Meta v1/v2, CES/KTE, frozen replay/experiments, detailed ledgers and offline trade review are integrated. Observational Trend/Regime/Structure/Volatility, Market Opportunities, Gann, broker DOM and Fed/ECB context have native surfaces. STOP/stale state withdraws quote plans; corrected history invalidates cached analysis.

Learning includes immutable completed-bar features, exact delayed labels, offline baseline training, challenger registry, chronological eligibility, prospective read-only inference, matured scoring and fixed performance-review policy. Native controls freeze model/evidence-bound reviews and load saved reports. Evidence v2 includes regime slices; policy thresholds remain v1. No Champion is created.

Schema-10 backups include research/learning state and exclude secrets. Validation checks exact members, version/schema, checksums/config/SQLite integrity without deleting unexpected sidecars. Restore requires STOP, stages a source and rollback copy, applies before SQLite startup under the Core lock, preserves local secrets and exposes the last result. Native Backup/Restore is implemented.

An earlier Windows GUI launch returned before rendering finished. The acceptance harness now waits for process exit and rejects any missing or incomplete frame; the final run above supersedes that incomplete evidence.

## Remaining work / next gate

Native acceptance on the user's own Windows laptop, actual MT5 broker/history/DOM and private Telegram credentials remains pending. Collect fresh out-of-sample scored observations before judging models; do not retune the same holdout after reviewing it.

Legacy Meta drives Shadow; other brains/learning remain observational. Broker order send/modify/close, broker execution/reconciliation, portfolio execution risk, true feed-supported order flow, cross-version/multi-account migration and Champion activation are separate future stages. No profitability guarantee is established.

Keep source on development, preserve newer commits, never overwrite from an old ZIP, and record the tested code boundary separately from documentation-only commits. Main is an older deliberately promoted checkpoint; branch protection is not enabled.
