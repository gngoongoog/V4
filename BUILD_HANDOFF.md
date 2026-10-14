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

### Learning Foundation v1

Learning Foundation compile-only run `38523080105` passed Domain + Runtime, Python gateway syntax and native Desktop compilation.

- `learning_snapshots` stores at most one immutable closed-bar feature snapshot per account/server/symbol/currency + M1 bar;
- feature schema `gold-m1-feature-v1` contains reproducible closed-bar Trend, Regime, Structure, Volatility, Meta-v2 candidate state, M1/M5/M15 momentum, Gann geometry, zones, setup metadata, tick-activity/profile proxies and session context;
- quote projection, DOM and News/Macro are deliberately excluded until replay parity exists for those feeds;
- `learning_labels` is written only after exact future M1 sequences exist for 5/15/30 minutes; gapped horizons are never shifted or fabricated;
- labels store ATR-normalized return, MFE, MAE and neutral/up/down class at ±0.20 ATR;
- `/v1/learning` exposes status and `/v1/learning/export` writes matured rows only in deterministic order;
- registry tables reserve `champion`/`challenger` but have no registered trained model by default;
- `training_enabled=false`, `prediction_enabled=false`, `live_self_modification=false`.

### Offline Linear Baseline Trainer — current increment

`scripts/train_linear_baseline.py` is now added as an offline-only research trainer and documented in `docs/LEARNING.md`.

- reads only a matured `dataset.jsonl` Learning Foundation export;
- verifies feature/label schema and hashes the exact dataset bytes with SHA-256;
- flattens a fixed transparent numerical feature set; no hidden feature search;
- uses chronological splits independently per account context;
- applies a 30-row purge on both sides of train/validation and validation/holdout boundaries because labels inspect up to 30 future M1 minutes;
- refuses datasets that remain too small after purging or whose partitions lack one of the three target classes;
- train-only standardization; no holdout normalization leakage;
- deterministic class-weighted 3-class softmax linear baseline with early stopping on validation log-loss;
- artifact records train, validation and untouched holdout accuracy, balanced accuracy, macro-F1, log-loss, mean confidence, confusion matrix and majority baseline;
- artifact status is always `unreviewed_challenger`, `eligible_for_promotion=false`; it has no MT5/Core network path and cannot affect Shadow.

The build-only gate now syntax-checks both `gateway/mt5_gateway.py` and `scripts/train_linear_baseline.py`. This commit requests that gate. It does not train a model because no user broker dataset was supplied in CI.

Automated strategy tests, profitability/effectiveness claims, native Windows packaging and laptop acceptance remain deferred by user instruction. Do not call the trainer output production AI or a profitable model before real chronological evaluation.

## Previous verified research baseline

Selectable historical Meta v2 and matched-data comparisons are implemented and verified. Tested source: `b488350416ccbbde1947ae3aae381ba02aec008d`.
Research CI: https://github.com/gngoongoog/V4/actions/runs/38516269617 — PASS.

## Next after the trainer compile/syntax gate

Add immutable model registration and challenger lifecycle only. Registration must verify schemas, dataset identity and artifact metadata; it must not enable inference. Then add manual promotion preconditions based on separately persisted chronological evaluation evidence. Only after registry and evaluation are sound should a read-only inference engine be designed; Shadow remains legacy Meta until explicit later promotion.

Keep live Shadow's source legacy Meta. No broker execution is enabled. Never commit credentials, private runtime databases/logs, transport chunks or ZIP-only source.
