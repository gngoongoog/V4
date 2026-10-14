# BrainTrade OS — Build Handoff

## Active task / user steering 2026-10-14

User explicitly deferred automated strategy testing and Windows packaging while the remaining analytical brains are being built. The current increment therefore uses compile-only gates.

## Current development checkpoint

Live market-opportunity integration is wired on `development` and its first compile-only gate passed in GitHub Actions run `38520989480`:

- closed-bar opportunity maps are cached per account/server/symbol/currency context and recomputed only when the completed M1 bar changes;
- demand/supply/support/resistance zones, confirmed swings, M1/M5/M15 momentum/divergence, tick-activity/profile proxies and session context feed grouped reversal/sweep/breakout/retest/value-reclaim setup discovery;
- each setup is repriced from the current quote with structural stop, three targets, spread/slippage/commission assumptions, broker lot constraints and a conservative risk budget;
- stale quotes expire price plans and stale/disconnected candle data withdraws them instead of leaving old Entry/SL/TP values visible;
- first-observed setup snapshots persist in SQLite table `opportunity_setups` using `(context, setup_id)` as stable identity;
- the Analysis page appends the read-only opportunity report;
- Telegram accepts `/opportunities`, `الفرص` and `فرص` and returns the same read-only setup report;
- no opportunity module has an order API and live Shadow still uses the existing legacy Meta decision source.

### Gann Geometry Brain v1

A new observational Gann-inspired geometry brain is now added to the market-specialist map. It uses only completed bars and confirmed pivots. It deliberately avoids chart-pixel angles: the latest confirmed swing is the time/price anchor, movement is normalized as ATR per bar, and the latest confirmed swing range is divided into eighths. M1/M5/M15 frames are exposed and a specialist vote is reported only when at least two directional frames agree. It does not change setup scoring, Meta, Shadow or broker execution.

This commit requests a second `[build-only]` gate for the Gann integration. Automated strategy tests, chronological evaluation and Windows packaging remain deferred for this increment.

Do not describe broker candle tick volume/profile proxies as exchange volume, DOM or global order flow. Do not describe Gann geometry strength as a win probability.

## Previous verified research baseline

Selectable historical Meta v2 and matched-data comparisons are implemented and verified. Tested source: `b488350416ccbbde1947ae3aae381ba02aec008d`.
Research CI: https://github.com/gngoongoog/V4/actions/runs/38516269617 — PASS.

- Rust domain: 38 PASS; runtime: 15 PASS, one explicit live-network test ignored; zero failures on that baseline.
- Python: 16 PASS. Actual Rust core build and smoke: PASS.
- Meta v2 Report/History/decisions.csv/comparison.json/HTML replay byte-identical on the verified research baseline.
- Native Slint desktop compilation passed on Linux for the verified baseline.
- Native MT5 history/Telegram delivery on the user's laptop: NOT RUN.

## Next after the Gann compile gate

Continue the analytical-brain roadmap without promoting any new brain into live trading. The next useful feed-boundary task is to separate what can be inferred from candle/tick proxies from true DOM/order-flow data and only enable a DOM brain when the broker actually supplies depth. After the analytical layer is complete, add chronological held-out/rolling evaluation and only then design the trained/self-learning model registry.

Keep live Shadow's source legacy Meta until real broker-data evaluation is reviewed. The user will perform native laptop/MT5/Telegram acceptance later. No broker execution is enabled. Never commit credentials, private runtime databases/logs, transport chunks or ZIP-only source.
