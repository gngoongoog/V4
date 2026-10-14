# BrainTrade OS — Build Handoff

Canonical repository: `gngoongoog/V4`. Stable branch: `main`; ongoing work:
`development`. Read this file, PROJECT_STATE.md, ROADMAP.md and
`docs/CONTINUITY_PROTOCOL.md` before editing. Never restore an old ZIP over GitHub.

## Last safe code checkpoint

Selectable historical Meta v2 and matched-data comparisons are implemented and
verified. Tested source: `b488350416ccbbde1947ae3aae381ba02aec008d`.
Research CI: https://github.com/gngoongoog/V4/actions/runs/38516269617 — PASS.
Application source is identical to 7555ab3d6ce14572f573eaef3630d6f99231620b; the
last code checkpoint adds the Linux fontconfig build dependency to CI only.
This final documentation checkpoint changes no executable files.
`main` remains at d9ca5d3; new work is on development. Windows packaging remains
deferred by user instruction; last delivered Windows portable is 0.2.0 Preview.

## What changed

- domain/backtest.rs: selectable `meta_v2`, independent algorithm identifier
  `meta-v2-candidate-v1 / M1-OHLC-conservative-v1`; same exit/cost/risk model as
  Meta v1, completed-bar candidate and admission audit for every tested minute.
- research.rs only re-exports MetaBrainV2; existing brain formulas and live Shadow
  decision source are unchanged.
- desktop Backtest selector: fourth strategy and accurate descriptions. Choose
  an existing Meta v1 run, select Meta v2, then experiment on that saved history.
- runtime/backtest.rs + backtest/comparison.rs: atomic experiment-parent storage
  in `backtest_experiments`; verify complete History and options except strategy
  before comparison; preserve parent on offline replay and restart.
- runtime/backtest/audit.rs: full decisions CSV, all-minute gate/outcome counts,
  latest 50 candidate rows in HTML; comparison metrics in desktop text and HTML,
  with `comparison.json`. Legacy reports omit empty `decisions` in their JSON.
- tests/scripts: causal prefix, data/shock/spread/size/gap/risk coverage; full
  legacy-report digests from verified pre-change synthetic CI artifacts;
  seven-run real-core persistence, exact replay and offline browser review.
- research.yml: Linux native selector compilation, including fontconfig headers.

## Verified on the tested source

- Rust domain: 38 PASS; runtime: 15 PASS, one explicit live-network test ignored;
  zero failures. GitHub Linux rustc 1.99.0.
- Python: 16 PASS. Actual Rust core build and smoke: PASS.
- Meta v2 audit: 2880 decisions on synthetic two-day sample; 262 admitted trades
  match the saved ledger. Same steady-trend sample produces identical Meta v1/v2
  metrics (delta net zero); this is a correctness fixture, not effectiveness evidence.
- Exact Meta/CES/KTE old-report digests PASS (only captured_ms normalized).
- Meta v2 Report/History/decisions.csv/comparison.json/HTML replay byte-identical;
  seven saved runs and parent comparison survive core restart.
- Mismatched costs, dates, bars, spread or contract cause comparison refusal.
- Browser: offline chart/ledger interactions, comparison rows and candidate audit,
  1200x900 / 880x900 layouts, no remote requests: PASS. Twelve synthetic images.
- Visual review: comparison at 880, candidate audit at 1200 and Meta v2 chart at
  880 inspected from 7555ab3 artifacts; same application code as tested b488350.
- Native Slint desktop `cargo check -p braintrade-desktop --locked`: PASS on Linux.
  This is compilation, not a native Windows/laptop interaction test.
- Local Python, Python/JS syntax and diff checks PASS. Local Rust: NOT RUN,
  no toolchain on PATH. Native MT5 history/Telegram delivery: NOT RUN.

Earlier failures were resolved: d715203's gap test incorrectly required invalid
analysis on the first observed gap minute instead of asserting blocked entry;
7555ab3 lacked fontconfig development headers for the new desktop CI gate.

## Next exact code task

Add an offline chronological evaluation path using saved History. Define explicit
in-sample/held-out or rolling windows, use warmup before each boundary without
permitting earlier entries, and independently close/reset each window's ledger.
Compare Meta v1/v2 on matching windows and costs. Persist split definitions,
algorithm/input identity and per-window/aggregate metrics. Test boundary isolation,
insufficient history, prefix causality and deterministic replay. No random split,
automatic parameter tuning or live strategy promotion in that increment.

Keep live Shadow's source legacy Meta until real broker-data evaluation is
reviewed. The user will perform native laptop/MT5/Telegram acceptance later.
No broker execution, real Telegram message or Windows package was produced.
Commit coherent batches to development and update this handoff. Never commit
credentials, private runtime databases/logs, transport chunks or ZIP-only source.
