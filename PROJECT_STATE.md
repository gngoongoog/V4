# BrainTrade OS — Project State

## Current checkpoint

- Development checkpoint: **0.4.0 in progress**.
- Stable branch: `main`.
- Active branch: `development`.
- Canonical repository: `gngoongoog/V4`.
- Last delivered Windows portable: **0.2.0 Preview**.
- A CI-generated Windows package is a validation artifact only; no new user-facing Windows release is declared until laptop/MT5 acceptance is explicitly performed.

## Implemented and preserved

- Native Rust Core + Slint desktop UI.
- Read-only MT5 gateway/bootstrap and account visibility.
- Start / Stop / reconnect / full-exit lifecycle controls.
- M1/M5/M15 completed-bar Meta analysis and shadow simulation.
- Persistent risk/shadow state and settings.
- Historical Meta/CES/KTE research lab with asynchronous MT5 history collection.
- UTC chunking, account isolation, cancellation, raw-input/report persistence.
- Deterministic offline replay and same-data strategy experiments.
- CSV / JSON / HTML exports with an equity curve.
- Official Fed/ECB RSS cache, freshness/error state and manual refresh.
- Telegram read-only `/news` and `/backtest` handlers.

## 0.4.0 KTE state on development

- KTE signal port based on the supplied defaults.
- Stateful KTE historical execution path.
- Three-leg partial handling: TP1 / TP2 / TP3.
- Break-even after TP1 and ATR trailing after TP2, effective only on subsequent bars.
- Conservative stop-first handling for unknown same-candle stop/target ordering.
- 45-minute pre-TP1 and 90-minute post-TP1 time limits.
- Three-following-bar cooldown after complete exit.
- Detailed ledger persisted in each `Trade`: fills, stop updates and target levels.
- Saved-report text, HTML report and SVG fill markers consume that same ledger.
- Detailed exports: `fills.csv`, `stop_updates.csv`, `targets.csv`; legacy `trades.csv` remains as a summary table.
- Offline replay checks detailed ledger exports byte-for-byte.

## Fresh verification

For the current 0.4.0 code path on Linux GitHub CI:

- braintrade-domain: **25 passed, 0 failed**.
- braintrade-runtime: **14 passed, 0 failed, 1 explicit live-network test ignored**.
- Python gateway/packaging: **16 passed**.
- `braintrade-core` build: **PASS**.
- real-core smoke/integration harness: **PASS**, including full export and deterministic detailed-ledger replay.

Windows CI packaging is still pending at this snapshot. The previous Windows failure was isolated to `os.fsync()` on a read-only temporary ZIP handle; the script now opens that handle read/write before `fsync`. Do not record Windows PASS until the current workflow actually completes successfully.

## Explicitly not complete

- Native MT5 historical acceptance against the user's broker/account.
- Live Telegram delivery with user credentials.
- Broker order execution/protection.
- Trained ML/ONNX.
- Gann brain.
- True order-flow/DOM acquisition and analysis.
- Volume profile.
- News-informed trade decisions.
- Walk-forward optimization.
- Portfolio/correlation risk manager.
- Full restore/recovery workflow.

No profitability, win-rate or HFT-latency claim is established by these software tests.

## Repository continuity

The project source is stored in `gngoongoog/V4` as normal GitHub files and folders. No `.import`, encoded staging, chunk transport or ZIP-only source storage belongs in the working tree. `Cargo.lock` is generated normally by Cargo. The permanent CI workflow is `.github/workflows/windows.yml`.

## Continuation rule

GitHub is the source of truth. Before new work, read `CONTINUE_HERE.md`, `BUILD_HANDOFF.md`, this file, `ROADMAP.md`, `CHANGELOG.md`, `docs/CONTINUITY_PROTOCOL.md`, and recent commits.

Commit ongoing work in coherent checkpoints to `development`. Keep `BUILD_HANDOFF.md` current with exact changed areas, test state and next action. Promote only a safe/resumable tested checkpoint to `main`.
