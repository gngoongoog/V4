# Live research / Shadow rules and current observational layers

BrainTrade's live Shadow path remains an untrained deterministic baseline. Learned challengers and newer analytical brains are observational research telemetry only.

## Legacy Meta / Shadow decision source

Closed M1/M5/M15 candles feed deterministic Trend, Momentum and Structure rules. Scores represent directional evidence, not win probability. Freshness, data completeness, spread, quote displacement, broker contract fields, existing MT5 exposure, virtual position state and risk limits can veto a virtual entry.

START permits new virtual entries; STOP blocks new entries while virtual exits continue while Core runs. Restart begins stopped. The Shadow ledger persists state and uses observed bid/ask plus configured costs. Missing price intervals are flagged instead of inventing intragap SL/TP fills.

No MT5 order, modify or close API is used.

## Observational brains and Market Opportunities

Current source also computes Trend, Regime, Structure, Volatility, Meta v2 candidate, Gann Geometry, structural zones/setups, broker-supported DOM Depth and Macro/News context. Market Opportunities can project a research entry/stop/three-target plan from completed-bar structure plus current quote and cost/lot gates.

These outputs do **not** replace legacy Meta in live Shadow and do not bypass deterministic risk.

Data caveats:
- MT5 candle tick volume is broker activity, not exchange traded volume.
- CFD Market Book/DOM is broker-specific resting depth and is not the worldwide gold order book.
- Fed/ECB RSS supplies official headline/context freshness; it is not converted directly into BUY/SELL.
- Stale/unavailable data withdraws evidence rather than generating synthetic observations.

## Historical and chronological research

The historical lab supports Meta v1, Meta v2 candidate, CES v1 and KTE v1 over saved broker candles with explicit simulation costs/limitations. Saved History and report ledgers support offline replay and experiments. A separate chronological evaluation service persists holdout/rolling Meta evidence against exact immutable source identity.

See `BACKTEST.md` and `META_V2.md` for engine details.

## Learning research boundary

The Learning Foundation captures immutable completed-bar feature snapshots, waits for exact future labels, exports deterministic datasets, supports an offline linear-softmax baseline, stores immutable challengers, requires manual review-eligibility evidence before read-only inference, and scores predictions only after exact matured labels exist.

Prediction metrics are descriptive out-of-sample audit evidence only. They do not feed `Analysis.action`, Meta, opportunities, risk, lot sizing, Shadow or broker execution. There is no champion activation/promotion route.

See `LEARNING.md` for the complete contract.

## Persistence

SQLite persists audit events, candles, Shadow state, historical runs, chronological evidence and learning audit data. Foreign keys are explicitly enabled and checked at startup. Schema-10 backups exclude secrets and support validated staged restart restore with rollback snapshots and native controls.

## Research claims

Synthetic fixtures and CI establish software behavior, determinism and contract integrity. They do not establish profitability, a future win rate, TradingView parity, worldwide order-flow visibility or real broker execution quality.
