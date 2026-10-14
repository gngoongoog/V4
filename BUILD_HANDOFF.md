# BrainTrade OS — Build Handoff

Canonical repository: `gngoongoog/V4`. Stable branch: `main`; ongoing work:
`development`. Read this file, PROJECT_STATE.md, ROADMAP.md and
`docs/CONTINUITY_PROTOCOL.md` before editing. Never restore an old ZIP over GitHub.

## Active task — Meta v2 historical evaluation (2026-10-14)

WIP checkpoint: selectable `meta_v2` / `meta-v2-candidate-v1` historical path
and per-opening candidate audit are implemented. Legacy admission and exit rules
are retained. Five new domain tests cover cost reconciliation, prefix causality,
missing/stale data, volatility/spread/size vetoes and gap/risk behavior. The native
selector includes Meta v2. Live Shadow is unchanged.

Local Python: 16 PASS; `git diff --check`: PASS. Rust tests and desktop compilation:
NOT RUN locally (no Rust on PATH); Linux research CI will validate this checkpoint.
A prior synthetic artifact was verified by SHA256 and reduced to three legacy
report digests in tests/legacy_report_hashes.json; integration assertions follow.

Next exact action: persist experiment parent linkage, display/export verified
same-data comparisons and candidate gates, wire the golden regression checks and
exercise Meta v2 replay/compare through the real core. Then inspect Linux CI,
fix any failures and update state documentation. Windows packaging is deferred.

## Previous verified code checkpoint

Offline per-trade candlestick review is implemented and verified.
Tested source commit: `7ea999e41a68a0aff264190f4dc6b79de0379a9b`.
Research CI: https://github.com/gngoongoog/V4/actions/runs/38473139804 — PASS.
The documentation checkpoint recording this result changes no executable files.
`main` remains at the earlier observational Meta v2 checkpoint d9ca5d3; report
review changes are saved on development. No promotion or Windows packaging was
performed in this increment.

## What changed

- crates/runtime/src/trade_review.html: self-contained offline M1 SVG chart,
  trade selector, signal/entry/target/fill markers, active stop segments, UTC
  labels, explicit gaps, costs and compact zero-trade state.
- crates/runtime/src/backtest.rs: embed exact saved History/Trade data in safe
  JSON, preserve the static ledger/CSVs and previous report calculations.
- scripts/check_backtest.py and smoke_core.py: a real KTE three-target fixture,
  five saved results, deterministic JSON/CSV/HTML replay and restart checks.
- scripts/check_trade_review.cjs: offline browser interactions, legacy fallback,
  empty state, 1200/880 layouts and synthetic screenshots.
- .github/workflows/research.yml: Linux-only research validation on development.
- State, roadmap, backtest and validation docs reconciled with current code.

Existing Trend/Regime, Structure/Volatility and Meta v2 remain observational.
Legacy Meta still drives Shadow entries. No strategy formulas, broker order
submission, Telegram sending or private user runtime data changed in this task.

## Verified on the tested source

- Rust domain: 33 passed; runtime: 14 passed; zero failed, one explicit network
  test ignored. Toolchain: rustc 1.99.0 on GitHub Linux.
- Python: 16 passed. Actual Rust core build and smoke: PASS.
- KTE fixture: TP1/TP2/TP3, stop ledger, fee/PnL reconciliation and identical
  offline CSV/JSON/HTML replay. Meta and empty CES reports also exercised.
- Browser: chart/ledger counts, selection, previous/next, empty and legacy
  behavior, no remote requests, at 1200x900 and 880x900: PASS.
- Six synthetic screenshots generated. KTE/Meta and compact empty state visually
  inspected; chart data and controls are readable.
- Local Python and JavaScript syntax checks passed. Local Rust was blocked before
  compilation by a truncated LLVM toolchain file; do not claim a local Rust pass.
- Native laptop/MT5 history and Telegram delivery: NOT RUN for these additions.
- Windows packaging: deferred by active user instruction. Last delivered portable
  remains 0.2.0 Preview; old CI-produced packages are validation artifacts only.

## Next exact code task

Implement a separately versioned selectable historical Meta v2 strategy, using
`docs/META_V2.md` and current `research/regime.rs`. Compare with legacy Meta on
identical frozen histories and costs. Preserve old algorithm replay, test causal
prefix stability and risk/gap handling, and persist candidate gates for review.
Keep live Shadow's decision source unchanged until this evaluation is reviewed.
The user can perform native laptop and Telegram acceptance later.

Commit each coherent batch on development and update this handoff with actual
verification and the next action. Never commit credentials, private databases,
encoded transport/chunks, `.import`, or ZIP-only source storage.
