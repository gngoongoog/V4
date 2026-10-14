# BrainTrade OS — Build Handoff

## Active task / user steering 2026-10-14

User requested that construction continue step by step with maximum precision. Windows packaging and native laptop acceptance remain deferred, but correctness validation is now appropriate for the chronological-evaluation boundary because future model promotion must not rely on unverified split logic. No profitability/effectiveness claim is implied by tests.

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

### Offline Linear Baseline Trainer

`scripts/train_linear_baseline.py` is an offline-only research trainer documented in `docs/LEARNING.md`.

- reads only a matured `dataset.jsonl` Learning Foundation export;
- verifies feature/label schema and hashes exact dataset bytes with SHA-256;
- flattens a fixed transparent numerical feature set; no hidden feature search;
- uses chronological splits independently per account context;
- applies a 30-row purge on both sides of train/validation and validation/holdout boundaries because labels inspect up to 30 future M1 minutes;
- refuses datasets too small after purging or partitions missing one of the three target classes;
- train-only standardization; no holdout normalization leakage;
- deterministic class-weighted 3-class softmax linear baseline with early stopping on validation log-loss;
- artifact records train, validation and untouched holdout accuracy, balanced accuracy, macro-F1, log-loss, mean confidence, confusion matrix and majority baseline;
- artifact status is always `unreviewed_challenger`, `eligible_for_promotion=false`; it has no MT5/Core network path and cannot affect Shadow.

### Chronological Evaluation Engine v1 — current increment

A domain-level chronological evaluator is now added on top of the existing deterministic `backtest::run` implementation instead of creating a second simulator.

- supports explicit `holdout` and rolling development/holdout plans; no random split;
- compares Meta v1 and Meta v2 using identical saved History, contract, costs and risk assumptions;
- every window creates a fresh ledger and resets virtual equity to the configured initial value; aggregate sums/averages are explicitly labelled as independent-window statistics, not a chained equity curve;
- at least two full days of real historical warmup are required before the first evaluation window;
- backtest entries remain bounded by each window's `from..to`; warmup can read only earlier completed bars;
- holdout and rolling definitions are UTC-day aligned and persisted in the result structure;
- targeted tests cover non-overlapping holdout boundaries, report bounds, rolling-window definitions, insufficient warmup/non-Meta rejection, and future mutation not changing an already completed development window;
- this increment is intentionally domain-only first. Runtime persistence/API and desktop review come only after the core split/causality tests pass.

This commit triggers the full research correctness workflow (Rust domain/runtime tests, core build/smoke, Python tests, offline browser review and native Desktop compile). Windows packaging remains deferred. Do not interpret CI success as profitability evidence.

## Previous verified research baseline

Selectable historical Meta v2 and matched-data comparisons are implemented and verified. Tested source: `b488350416ccbbde1947ae3aae381ba02aec008d`.
Research CI: https://github.com/gngoongoog/V4/actions/runs/38516269617 — PASS.

## Next exact action

1. Fix any chronological-evaluation compile/test failure without weakening causality or boundary rules.
2. Once green, add runtime persistence tied to an exact saved `backtest_runs` source id and expose create/read/export API routes.
3. Persist evaluation evidence immutably with source History identity, plan, algorithms and per-window metrics.
4. Add desktop review for development vs holdout/rolling windows.
5. Then add immutable model registration/challenger lifecycle. Registration must verify schemas, dataset SHA-256 and artifact metadata; it must not enable inference.
6. Manual promotion preconditions must reference separately persisted chronological evaluation evidence. Only after registry + evaluation are sound should read-only inference be designed.

Keep live Shadow's source legacy Meta. No broker execution is enabled. `training_enabled=false`, `prediction_enabled=false`, `live_self_modification=false`. Never commit credentials, private runtime databases/logs, transport chunks or ZIP-only source.