# BrainTrade OS — Build Handoff

Canonical source: `gngoongoog/V4`. Continue on `development`; `main` is the stable branch.
Read this file, PROJECT_STATE.md, ROADMAP.md and docs/CONTINUITY_PROTOCOL.md first.

## Active checkpoint — trade candlestick review (WIP)

Resumed from development `d9ca5d356d75e042fb1c65b026344e2695f9f7e4`.
This base already contains KTE partial-fill validation, independent Trend/Regime,
Structure/Volatility and observational Meta v2. Preserve these newer changes.
The earlier ZIP and old pending-Windows handoff are stale. GitHub is authoritative.

Current change: extend the saved historical HTML report with an offline per-trade
M1 candlestick chart. It reads the exact saved History and Trade ledger: entries,
targets, fills, effective stop updates, visible gaps, costs and UTC times.
Legacy Meta/CES reports receive a display-only single-exit fallback. No strategy,
Shadow entry decision, broker execution or saved report calculation is changed.

## Changed files

- crates/runtime/src/trade_review.html: local SVG/DOM chart, selector and ledger.
- crates/runtime/src/backtest.rs: safe embedded JSON and history-backed export.
- BUILD_HANDOFF.md: current continuation checkpoint.

## Validation

Research CI on `5288368d375a16d834cd2fa57260c0fc3d7cd300` passed:
https://github.com/gngoongoog/V4/actions/runs/38472670478
- Rust: domain 33 passed, runtime 14 passed, one explicit network test ignored.
- Python: 16 passed; core build and real-core smoke passed.
- Real KTE fixture: three targets, saved fills/stops, deterministic JSON/CSV/HTML replay.
- Browser: offline interactions, legacy fallback and zero trades at 1200/880 widths.
- Six synthetic screenshots were produced; KTE/Meta/empty states visually inspected.

Visual review found unnecessary blank chart space in zero-trade reports. The latest
small adjustment hides that panel and prevents section heading margin collapse.
This adjustment still needs its new CI run checked before final validation.
Local Rust compilation was blocked by a truncated toolchain LLVM file; all Rust
results above came from GitHub CI, not that failed local installation.
Windows packaging stays deferred; native laptop MT5/Telegram remain NOT RUN.

## Next exact action

Check the research workflow after the empty-state correction. Inspect the refreshed
KTE and empty-state screenshots. Reconcile state docs and save a final tested
checkpoint on development. Do not trigger Windows packaging or promote main.

After trade review is complete, the next code task is a selectable historical
Meta v2 evaluation path (docs/META_V2.md). Keep legacy Meta as Shadow's decision
source until identical-data comparison exists. Native laptop tests can wait.
