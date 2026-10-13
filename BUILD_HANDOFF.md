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

- Base candidate PR CI: success on `adf94f8ede354a15db350954557dc7fc00afba87`,
  run https://github.com/gngoongoog/V4/actions/runs/38460931318.
- New code Rust tests: RUNNING / not yet recorded as passed.
- Browser interaction and real-core export integration: NOT RUN yet.
- Native laptop MT5 and live Telegram: NOT RUN.
- Windows packaging: deferred by the user's active instruction. Do not start it.

## Next exact action

Finish the local Rust test run, extend the real-core synthetic fixture to create
an actual KTE three-target report, and test the generated HTML in a local headless
browser at desktop and narrow sizes (including no-trades and legacy reports).
Update this handoff and reconcile PROJECT_STATE/ROADMAP/CHANGELOG with actual
code and verification. Save another small checkpoint to development. Do not
promote to main or declare native broker acceptance from a fixture test.

After trade review is complete, the next code task is a selectable historical
Meta v2 evaluation path (docs/META_V2.md). Keep legacy Meta as Shadow's decision
source until identical-data comparison exists. Native laptop tests can wait.
