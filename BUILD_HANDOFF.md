# BrainTrade OS — Build Handoff

## Active task / user steering 2026-10-14

User explicitly deferred automated strategy testing and Windows packaging while the remaining analytical brains are being built. Current increments therefore use compile-only gates.

## Current development checkpoint

### Market Opportunity Engine

Live market-opportunity integration is wired on `development`; compile-only run `38520989480` passed Domain + Runtime + native Desktop compilation.

- closed-bar opportunity maps are cached per account/server/symbol/currency context and recomputed only when the completed M1 bar changes;
- demand/supply/support/resistance zones, confirmed swings, M1/M5/M15 momentum/divergence, tick-activity/profile proxies and session context feed grouped reversal/sweep/breakout/retest/value-reclaim setup discovery;
- each setup is repriced from the current quote with structural stop, three targets, spread/slippage/commission assumptions, broker lot constraints and a conservative risk budget;
- stale quotes expire price plans and stale/disconnected candle data withdraw old Entry/SL/TP values;
- first-observed setup snapshots persist in `opportunity_setups` using `(context, setup_id)` identity;
- Analysis dashboard and Telegram `/opportunities` expose the same read-only setup report;
- no opportunity module has an order API and live Shadow still uses legacy Meta.

### Gann Geometry Brain v1

Gann-inspired geometry is integrated and compile-only run `38521328784` passed Domain + Runtime + native Desktop compilation. It uses completed bars and confirmed pivots only: latest confirmed swing as time/price anchor, ATR-per-bar normalized slope, and the latest confirmed swing range split into eighths. M1/M5/M15 are exposed; it remains observational and does not change setup scoring, Meta or Shadow.

### DOM Depth Brain v1

Broker-supported MT5 Market Book depth is integrated and compile-only run `38521812267` passed Domain + Runtime, Python gateway syntax and native Desktop compilation.

- Telemetry validates `dom` rows (`buy`/`sell`, positive finite price/volume, max 200);
- gateway subscribes with `market_book_add`, reads `market_book_get`, releases on context changes/disconnect/shutdown and sends at most 100 validated rows;
- DOM Brain uses nearest five bid/ask levels and exposes visible resting depth, imbalance, best bid/ask and microprice proxy;
- it does NOT claim executed order flow, footprint delta, hidden liquidity or exchange-wide volume;
- DOM is shown as an observational specialist and does not alter setup scoring, Meta, Shadow or execution.

### Macro / News Context Brain v1

Macro/News integration compile-only run `38522309782` passed Domain + Runtime, Python gateway syntax and native Desktop compilation.

- official Fed/ECB source freshness is mandatory; failed/stale feeds are not treated as current context;
- counts headlines inside 15m and 60m windows and highlights monetary-policy/inflation/employment topics published within 30m;
- state is `STALE`, `QUIET`, `RECENT_NEWS` or `ATTENTION`;
- never derives BUY/SELL or sentiment from headline text and does not pretend RSS is a future economic calendar;
- News dashboard and Telegram `/news` expose the context;
- Meta, Shadow, historical strategy decisions and broker execution remain unchanged.

### Learning Foundation v1 — current increment

A leakage-aware learning data foundation is now wired into Core, with no trained model or prediction path:

- `learning_snapshots` stores at most one immutable closed-bar feature snapshot per account/server/symbol/currency + M1 bar;
- feature schema is `gold-m1-feature-v1` and uses reproducible closed-bar data only: Trend, Regime, Structure, Volatility, Meta-v2 candidate state, M1/M5/M15 momentum, Gann geometry, zone geometry, setup metadata, tick-activity/profile proxies and session context;
- live quote projection, DOM and News/Macro are deliberately excluded from v1 training features because historical replay parity for those feeds is not yet available;
- labels are never written immediately. `learning_labels` is inserted only after exact future M1 sequences exist for 5/15/30 minutes;
- each horizon stores final ATR-normalized return, MFE, MAE and a neutral/up/down class using ±0.20 ATR; missing or gapped future minutes are not shifted or fabricated;
- model registry tables exist (`learning_models`, `learning_slots`) with reserved `champion` and `challenger` slots, but contain no trained model by default;
- `/v1/learning` exposes dataset/registry status; `/v1/learning/export` writes matured rows only in deterministic order;
- backup schema now includes learning dataset/registry metadata while explicitly declaring `trained_model_included=false`;
- `training_enabled=false`, `prediction_enabled=false` and `live_self_modification=false` remain explicit invariants.

This commit requests a `[build-only]` gate for Learning Foundation v1. Automated strategy tests, statistical effectiveness claims, chronological model evaluation and Windows packaging remain deferred by user instruction.

Do not describe candle tick-volume/profile proxies as exchange volume or order flow. Do not describe Gann/DOM/Macro evidence as a win probability. Do not call the learning foundation an AI model until an actual versioned trained artifact exists.

## Previous verified research baseline

Selectable historical Meta v2 and matched-data comparisons are implemented and verified. Tested source: `b488350416ccbbde1947ae3aae381ba02aec008d`.
Research CI: https://github.com/gngoongoog/V4/actions/runs/38516269617 — PASS.

- Rust domain: 38 PASS; runtime: 15 PASS, one explicit live-network test ignored; zero failures on that baseline.
- Python: 16 PASS. Actual Rust core build and smoke: PASS.
- Meta v2 Report/History/decisions.csv/comparison.json/HTML replay byte-identical on the verified research baseline.
- Native Slint desktop compilation passed on Linux for the verified baseline.
- Native MT5 history/Telegram/DOM delivery on the user's laptop: NOT RUN.

## Next after the Learning Foundation compile gate

If compilation passes, build the offline trainer contract and model registry operations without allowing prediction to affect Shadow: deterministic dataset identity, explicit train/validation/holdout windows, simple interpretable baseline model first, metrics persisted with model metadata, then challenger registration. Champion promotion must remain manual and impossible unless chronological held-out evaluation criteria are satisfied. Only after this foundation is validated should ONNX/runtime inference be added.

Keep live Shadow's source legacy Meta until real broker-data evaluation is reviewed. The user will perform native laptop/MT5/Telegram acceptance later. No broker execution is enabled. Never commit credentials, private runtime databases/logs, transport chunks or ZIP-only source.
