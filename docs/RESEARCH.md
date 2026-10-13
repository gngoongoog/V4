# Live Meta / Shadow — exact rules and limits (retained in 0.3.0)

This is an untrained, deterministic baseline, not a demonstrated profitable
system. Three rule families produce five votes: Trend M1/M5/M15, Momentum M1,
and Structure M1. Scores measure directional evidence, never win probability.

## Market data

MT5 snapshots poll once per second. A per-account/server/symbol cache retrieves
180 completed candles on M1/M5/M15 every five seconds (start_pos=1 excludes the
forming candle). Require 100 bars per timeframe, valid OHLC, increasing UTC times,
30 recent consecutive intervals, no unfinished/future bar and no stale close
(older than one timeframe +20s). Cache expires at 15s; executable quotes at 3s.
After a weekend, 30 consecutive M15 bars may require 7.5 hours. Missing/closed
market data yields WAIT, never fabricated history. Tick volume is broker activity,
not exchange order flow or a volume-profile source.

## Agents

ATR = simple mean of the latest 14 true ranges (not Wilder smoothing).
EMA is seeded from the first close in the available window. Trend requires
EMA20/EMA50 separation >0.15 ATR and a matching EMA20 slope over 3 bars >0.03 ATR.
Evidence=min(abs(separation/ATR),1).
Momentum=(latest close-close five bars ago)/ATR, threshold +/-0.50; evidence is
min(abs(momentum)/2,1). Structure compares the latest close with the previous
20-bar high/low range, excluding the latest bar, using a 0.05 ATR breakout buffer.
Meta requires all three trends aligned, momentum or structure confirming, and
neither trigger opposing. These votes are correlated rules, not independent ML.

## Shadow execution and risk

START permits new virtual entries; STOP blocks entries but virtual exits keep
running on observed quotes while Core runs. Restart always starts stopped.
No MT5 order/modify/close API is used. Existing MT5 positions/orders veto new
virtual entries. Other vetoes: stale quotes, spread above the chosen ATR fraction,
quote displacement >2 ATR from analyzed close, missing broker volume/tick specs,
minimum lot above budget, existing virtual position, daily loss or peak drawdown.
Only one decision per completed M1 bar per account/server/symbol/currency. The
bar key and open virtual position survive restart. No re-entry on an exit quote.

BUY entry=ask+slippage, SELL entry=bid-slippage. SL=1.5 ATR rounded outward to tick;
TP=2x rounded stop distance, also outward. Budget uses the smaller of actual
account equity and closed virtual equity times configured risk%, capped by
remaining daily/peak budgets. Loss estimate includes stop distance, exit slippage,
the larger broker profit/loss tick value and round-trip commission. Volume is
floored to volume_step and capped by configured max_lots and broker volume_max;
reject if below volume_min. This is a linear tick-value approximation, not exact
order_calc_profit/margin preflight and not a live-execution risk engine.

Virtual equity seeds from actual equity once per account/server/symbol/currency;
subsequent deposits do not reset its PnL. Daily loss starts at first observed
virtual mark of the Baghdad UTC+03 day, not exact midnight when data is absent.
Daily latch resets next observed day; peak-drawdown latch has no reset UI.
Changing settings does not erase history or clear locks.

BUY exits use bid-slippage; SELL exits ask+slippage. Observed bid/ask triggers
SL/TP; overshoot is retained. Timeout=30 minutes, closed on the next observed
quote. Configured full round-trip commission is reserved in open PnL and charged
once on close. Tick values/cost assumptions freeze at entry. Swaps, partial fills,
future currency-conversion changes and actual broker execution are not modeled.
No retroactive high/low fills are inferred. Quote gaps >8s flag a trade interrupted;
it resumes at first returned quote, with missed intragap exits explicitly unknown.
Inactive account/symbol books do not observe prices; revisiting marks gaps.
This live shadow ledger is not a multi-account portfolio manager. The separate historical tester is documented in BACKTEST.md.

SQLite atomically saves candle inserts, decision/risk assumptions, virtual events
and checkpoint; only then does the core publish the new in-memory state.
Record first observed closed-bar values; later broker revisions do not overwrite.
Recording is bounded by available 180-bar windows on connection, not a full tick
tape. No retention job yet. JSONL export includes all stored records and account
identity. Backup includes database and config, excludes secrets; restore UI pending.

Official API sources:
- https://www.mql5.com/en/docs/python_metatrader5/mt5copyratesfrompos_py
- https://www.mql5.com/en/docs/python_metatrader5/mt5symbolinfo_py
- https://www.mql5.com/en/docs/python_metatrader5/mt5ordercalcprofit_py

Historical replay and CES are documented in BACKTEST.md; official headlines in NEWS.md.
Future work includes walk-forward evaluation, KTE/Gann, true order flow, trained
models, news-informed decisions, native tick adapter and reconciled order state
machine. No latency, profitability or accuracy claim is made.
