# BrainTrade OS — Project State

Canonical repository: `gngoongoog/V4`. Stable branch: `main`; active work:
`development`. The 0.4.0 research milestone is in progress; Cargo/package labels
still use 0.3.0. No new end-user Windows release is declared. Last delivered
portable: 0.2.0 Preview. Native broker acceptance remains separate from CI.

## Implemented

- Independent Rust Core, native Slint dashboard, Start/Stop/reconnect/full exit.
- Read-only MT5 bootstrap, account visibility and completed M1/M5/M15 data.
- Persistent Shadow simulation/risk and legacy Meta entry decisions.
- Historical Meta v1/v2/CES/KTE lab: asynchronous UTC chunks, isolation, cancellation,
  exact input/report storage, offline replay and same-data experiments.
- KTE three-leg partial fills, next-minute break-even/trailing, time limits,
  stop-first ambiguity and three-following-bar cooldown.
- One saved ledger drives report text, fills/stop-updates/targets CSV, JSON and
  HTML; new per-trade candlestick review reads the same history and ledger.
- Trend/Regime v1, Structure/Volatility v1 and Meta v2 candidate outputs.
  They remain observational in live analysis. Historical Meta v2 is selectable,
  stores candidate gates/admission outcomes, and uses Meta v1's cost/risk model.
- Same-data experiments persist parent linkage and verify complete history and
  option equality. Comparison metrics and candidate CSV/JSON/HTML survive replay
  and restart. Legacy Meta remains Shadow's entry source.
- Fed/ECB RSS cache/freshness controls and private read-only Telegram queries.

## Current verification

Tested source: `b488350416ccbbde1947ae3aae381ba02aec008d`. [Linux research CI](https://github.com/gngoongoog/V4/actions/runs/38516269617) passed:
38 domain + 15 runtime Rust tests (one live-network test ignored), 16 Python tests,
core build/smoke, seven saved runs with comparison persistence, full legacy-report
digests, deterministic candidate CSV/JSON/HTML replay and browser review at
1200/880 widths. Native Slint desktop `cargo check` also passed on Linux.
The final state documentation checkpoint changes no executable code.
Native Windows/MT5/Telegram acceptance remains pending. See BUILD_HANDOFF.md
and docs/validation.json for the verification boundary.

## Remaining

Real broker-data Meta v1/v2 evaluation and chronological out-of-sample/walk-forward
research remain pending. Native laptop/MT5 history
acceptance and Telegram delivery with local credentials have not been done for
these additions. Broker execution, Gann, actual order flow/DOM, volume profile,
news-informed decisions, walk-forward, trained ML/ONNX, portfolio risk and full
restore remain roadmap work. No profitability or latency claim is established.

## Continuity

Keep source as normal Git files. Commit small coherent checkpoints to development
and update BUILD_HANDOFF.md. Never reconstruct over newer code from an old ZIP.
Never commit credentials, tokens, private runtime databases or personal logs.
