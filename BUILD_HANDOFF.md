# BrainTrade OS — Build Handoff

## Active task / user steering 2026-10-14

User explicitly deferred automated strategy testing and Windows packaging while the remaining analytical brains are being built. The current increment therefore uses a compile-only gate.

### Market opportunity integration checkpoint

The new market-specialist module is now wired into live research on `development`:

- closed-bar opportunity maps are cached per account/server/symbol/currency context and recomputed only when the completed M1 bar changes;
- demand/supply/support/resistance zones, confirmed swings, M1/M5/M15 momentum/divergence, tick-activity/profile proxies and session context feed grouped reversal/sweep/breakout/retest/value-reclaim setup discovery;
- each setup is repriced from the current quote with structural stop, three targets, spread/slippage/commission assumptions, broker lot constraints and a conservative risk budget;
- stale quotes expire price plans and stale/disconnected candle data withdraws them instead of leaving old Entry/SL/TP values visible;
- first-observed setup snapshots are persisted in SQLite table `opportunity_setups` using `(context, setup_id)` as the stable identity;
- the existing Analysis page now appends the read-only opportunity report;
- Telegram accepts `/opportunities`, `الفرص` and `فرص` and returns the same read-only setup report;
- no opportunity module has an order API and live Shadow still uses the existing legacy Meta decision source.

A `[build-only]` CI gate is requested by this commit. It checks Rust domain/runtime and native desktop compilation only. It intentionally does not run chronological/backtest validation or Windows packaging in this increment.

Do not describe broker candle tick volume/profile proxies as exchange volume, DOM or global order flow. These are bounded MT5 data proxies only.

## Previous verified baseline

Selectable historical Meta v2 and matched-data comparisons are implemented and verified. Tested source: `b488350416ccbbde1947ae3aae381ba02aec008d`.
Research CI: https://github.com/gngoongoog/V4/actions/runs/38516269617 — PASS.
Application source is identical to 7555ab3d6ce14572f573eaef3630d6f99231620b; the last code checkpoint adds the Linux fontconfig build dependency to CI only.
`main` remains the stable branch; new work stays on `development`. Windows packaging remains deferred by user instruction; last delivered Windows portable is 0.2.0 Preview.

## Verified on the previous tested source

- Rust domain: 38 PASS; runtime: 15 PASS, one explicit live-network test ignored; zero failures. GitHub Linux rustc 1.99.0.
- Python: 16 PASS. Actual Rust core build and smoke: PASS.
- Meta v2 audit: 2880 decisions on synthetic two-day sample; 262 admitted trades match the saved ledger. Same steady-trend sample produces identical Meta v1/v2 metrics (delta net zero); this is a correctness fixture, not effectiveness evidence.
- Exact Meta/CES/KTE old-report digests PASS (only captured_ms normalized).
- Meta v2 Report/History/decisions.csv/comparison.json/HTML replay byte-identical; seven saved runs and parent comparison survive core restart.
- Mismatched costs, dates, bars, spread or contract cause comparison refusal.
- Browser: offline chart/ledger interactions, comparison rows and candidate audit, 1200x900 / 880x900 layouts, no remote requests: PASS.
- Native Slint desktop `cargo check -p braintrade-desktop --locked`: PASS on Linux for the previous source.
- Native MT5 history/Telegram delivery on the user's laptop: NOT RUN.

## Next after the compile gate

If compilation succeeds, record the exact CI run and continue with the remaining analytical-brain roadmap. The opportunity module remains observational until later chronological evaluation on saved broker history. The planned evaluation must use explicit held-out/rolling windows, warmup before each boundary without allowing earlier entries, independent per-window ledgers, deterministic replay and no random split or automatic strategy promotion.

Keep live Shadow's source legacy Meta until real broker-data evaluation is reviewed. The user will perform native laptop/MT5/Telegram acceptance later. No broker execution is enabled. Never commit credentials, private runtime databases/logs, transport chunks or ZIP-only source.
