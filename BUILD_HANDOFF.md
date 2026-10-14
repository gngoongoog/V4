# BrainTrade OS — Build Handoff

## Active task / user steering 2026-10-14

User explicitly deferred automated strategy testing and Windows packaging while the remaining analytical brains are being built. The current increment therefore uses compile-only gates.

## Current development checkpoint

### Market Opportunity Engine

Live market-opportunity integration is wired on `development`; compile-only run `38520989480` passed Domain + Runtime + native Desktop compilation.

- closed-bar opportunity maps are cached per account/server/symbol/currency context and recomputed only when the completed M1 bar changes;
- demand/supply/support/resistance zones, confirmed swings, M1/M5/M15 momentum/divergence, tick-activity/profile proxies and session context feed grouped reversal/sweep/breakout/retest/value-reclaim setup discovery;
- each setup is repriced from the current quote with structural stop, three targets, spread/slippage/commission assumptions, broker lot constraints and a conservative risk budget;
- stale quotes expire price plans and stale/disconnected candle data withdraws old Entry/SL/TP values;
- first-observed setup snapshots persist in `opportunity_setups` using `(context, setup_id)` identity;
- Analysis dashboard and Telegram `/opportunities` expose the same read-only setup report;
- no opportunity module has an order API and live Shadow still uses legacy Meta.

### Gann Geometry Brain v1

Gann-inspired geometry is integrated and compile-only run `38521328784` passed Domain + Runtime + native Desktop compilation. It uses completed bars and confirmed pivots only: latest confirmed swing as time/price anchor, ATR-per-bar normalized slope, and the latest confirmed swing range split into eighths. M1/M5/M15 are exposed; it remains observational and does not change setup scoring, Meta or Shadow.

### DOM Depth Brain v1 — current increment

True broker-supported MT5 Market Book depth is now being integrated without inventing order flow:

- Telemetry has validated `dom` levels (`buy`/`sell`, positive finite price/volume, max 200 rows) plus `dom_available`;
- Python gateway subscribes using `market_book_add`, reads `market_book_get`, releases subscriptions on account/symbol changes and disconnect/shutdown, and sends at most 100 validated book rows;
- DOM Brain uses the nearest five bid/ask levels, exposes visible resting bid/ask depth, imbalance, best bid/ask and a microprice proxy;
- it explicitly does NOT claim executed order flow, footprint delta, hidden liquidity or exchange-wide volume;
- Market Opportunities displays DOM state and adds an observational specialist vote; DOM does not yet alter setup scoring or trading decisions;
- build-only CI now also syntax-checks `gateway/mt5_gateway.py` with Python 3.13.

This commit requests a `[build-only]` gate for the complete DOM integration. Automated strategy tests, chronological evaluation and Windows packaging remain deferred for this increment.

Do not describe broker candle tick volume/profile proxies as exchange volume, DOM or global order flow. Do not describe Gann/DOM evidence as win probability.

## Previous verified research baseline

Selectable historical Meta v2 and matched-data comparisons are implemented and verified. Tested source: `b488350416ccbbde1947ae3aae381ba02aec008d`.
Research CI: https://github.com/gngoongoog/V4/actions/runs/38516269617 — PASS.

- Rust domain: 38 PASS; runtime: 15 PASS, one explicit live-network test ignored; zero failures on that baseline.
- Python: 16 PASS. Actual Rust core build and smoke: PASS.
- Meta v2 Report/History/decisions.csv/comparison.json/HTML replay byte-identical on the verified research baseline.
- Native Slint desktop compilation passed on Linux for the verified baseline.
- Native MT5 history/Telegram/DOM delivery on the user's laptop: NOT RUN.

## Next after the DOM compile gate

Continue the analytical-brain roadmap without promoting new brains to live execution. Next priorities are macro/news context and completing data-quality/feed capability boundaries. After the analytical layer is coherent, add chronological held-out/rolling evaluation, then build a trained/self-learning model registry with offline training, champion/challenger promotion and no uncontrolled live self-modification.

Keep live Shadow's source legacy Meta until real broker-data evaluation is reviewed. The user will perform native laptop/MT5/Telegram acceptance later. No broker execution is enabled. Never commit credentials, private runtime databases/logs, transport chunks or ZIP-only source.
