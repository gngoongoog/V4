# BrainTrade OS — Build Handoff

Canonical repository: `gngoongoog/V4`. Stable branch: `main`; ongoing work:
`development`. Read this file, PROJECT_STATE.md, ROADMAP.md and
`docs/CONTINUITY_PROTOCOL.md` before editing. Never restore an old ZIP over GitHub.

## Active task — Meta v2 historical evaluation (2026-10-14)

WIP checkpoint: historical Meta v2, per-opening candidate audit, experiment
lineage, matched-data/cost comparison and desktop selector are implemented.
New comparison metadata is stored atomically alongside a completed report;
offline replay preserves its baseline. JSON/CSV/HTML and report text expose the
candidate gates and comparison. Live Shadow remains legacy Meta.

First checkpoint d715203 compiled on Linux. Domain: 37 PASS / 1 FAIL; the failed
gap test incorrectly required `current=false` on the first observed gap minute,
where the ledger already rejects entry. It now checks that admission is blocked
and subsequent incomplete windows are unavailable. No strategy rule changed.
Current tests: NOT RUN in CI yet. Local Python 16 PASS; Python/JS syntax and diff
checks PASS. No Rust installed locally. No Windows build/package triggered.

Integration now checks seven persisted results, full legacy-report golden digests
from prior verified synthetic artifacts, Meta v2 decisions CSV, same-data comparison,
byte-identical offline replay and comparison survival across restart. Linux CI
also checks the native Slint selector compilation; browser review adds Meta v2.

Next exact action: inspect current Linux research CI, fix failures, verify the
comparison/audit screenshots and update PROJECT_STATE, ROADMAP, CHANGELOG,
docs/META_V2.md, docs/BACKTEST.md and validation.json with the actual result.
Then record the next research step, keeping native broker acceptance pending.

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
