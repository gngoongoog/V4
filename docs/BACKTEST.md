# Historical research — 0.4.0 development

The native Backtest page supports Meta v1, CES v1 and KTE v1. Live quote-sampled Shadow continues to use Meta. Historical testing never submits broker orders.

## Run and replay

Choose UTC start/end dates, one to 30 calendar days; the end date is exclusive. Choose virtual equity in the account currency and a spread floor in price units. The example 0.20 is for a gold-like symbol and must be changed for other markets. Risk settings are frozen when starting the job. Press STOP and let the current virtual position close before fetching history, so history reads do not interrupt an active simulation. Current MT5 broker positions are read-only and unaffected.

The Python gateway requests completed M1 bars using timezone-aware UTC dates and `copy_rates_range`, one day per cycle, including seven calendar days of warmup. The MT5 terminal must have the history available; its Max bars setting and broker coverage can limit results. Empty periods are not fabricated. Partial coverage and time gaps appear in each report. Account, server, symbol, currency and point must match the frozen request; identity changes reject the chunk. Cancelling rejects late chunks without damaging a newer job.

The core worker runs independently of the UI. Reports and exact input bars are stored together in SQLite before a job is marked completed. A saved run can be opened, exported or replayed offline after restart. Replay freezes the original strategy, equity, dates, costs and risk. Experiment uses the selected strategy on the selected saved run's exact data and assumptions, creating a separate run; editing the form's equity/spread does not change an offline experiment. This is not walk-forward testing or a parameter optimizer.

## Report and export invariants

`report.json` is the canonical serialized `Report`. `trades.csv` keeps one summary row per completed trade for compatibility. Detailed execution state comes from the same saved `Trade` objects and is exported as:

- `fills.csv` — each partial/final fill with time, phase, price, lots, commission, net result, reason and ambiguity flag;
- `stop_updates.csv` — initial stop plus later break-even / trailing changes and their effective times;
- `targets.csv` — the saved target prices for each trade;
- `equity.csv` — minute equity/balance points;
- `history.json` — exact historical input bars;
- `report.html` — local self-contained report using the same ledger, including fill markers on the equity SVG and a readable KTE ledger section.

Offline replay must reproduce the detailed ledger exports byte-for-byte when strategy and inputs are unchanged. CSV fields that may contain commas or quotes are quoted. The desktop saved-report view also prints the same fills, stop updates and target ledger instead of reconstructing a second simplified KTE result.

## Common fill model

- Signal features use only candles closed before the entry minute. Meta needs complete M5/M15 groups aggregated from M1; no missing bars are interpolated.
- BUY enters ask plus configured slippage; SELL enters bid minus slippage. Ask is approximated by a constant spread for each minute: the larger of the user floor and the PREVIOUS completed M1 spread times symbol point.
- BUY exits use bid; SELL exits use ask. Slippage is charged at fills and configured commission is accounted in simulated PnL. Minimum lot is never rounded upward through the risk budget; broker step and maximum lot are respected.
- One virtual position at a time. If stop and an unfilled target are both inside the same unknown intrabar OHLC range, the conservative path gives the stop priority and marks the trade ambiguous. A stop crossed at the open fills at that worse open. Missing-minute gaps close an existing position at the first observed open with a `data_gap` reason; no intermediate fill is invented.
- Daily loss resets on Baghdad day boundaries; maximum drawdown latches for the run. Equity is marked at open, close and fills, not every unknown intrabar excursion.
- Current tick values/contract specifications are frozen. Historical currency conversion, margin, swaps and broker execution are not modeled. Zero-trade runs and undefined profit factors are stated explicitly, not treated as wins.

## KTE v1 historical execution

KTE v1 is a stateful historical strategy. The signal port uses the supplied KTE defaults and executes only on completed information. When a position is accepted, its total lot is split deterministically into three valid broker-step legs (40% / 30% / remainder, normally 30%); if the position cannot form three legs at or above minimum volume, the entry is skipped.

The saved trade carries `fills`, `stop_updates` and `targets` as its execution ledger. TP1 closes the first leg and allows break-even to become active from the next minute. TP2 closes the second leg and enables the ATR trailing stop for subsequent minutes. TP3 closes the final leg. Before TP1, the time limit is 45 minutes; after TP1 it is 90 minutes. A complete exit starts a three-following-bar cooldown before another KTE entry can be accepted.

The OHLC model cannot know the order of events inside one candle. Therefore if the active stop and an unfilled target are both reachable in the same range, stop-first is used and `ambiguous=true` records that conservative assumption. Break-even and ATR trailing updates have an `effective_time`; they are not applied retroactively to the candle that generated them.

Regression coverage checks deterministic 40/30/30 splitting, TP1→break-even, TP2→ATR trail, TP3 completion, conservative stop/target ambiguity, 45/90-minute time limits, exact three-bar cooldown, and consistency between the saved ledger, UI text, CSV exports and HTML fill markers. These are software correctness tests on synthetic fixtures, not evidence of profitability.

## CES default implementation

Source: the user's `CES_v1_Critical_Entropy_Scalper.pine`, read from its current saved contents. That file defines EPI using VOLUME, despite a candle-size variant in earlier discussions. This port follows the file and substitutes MT5 tick volume, not exchange traded volume. No claim of TradingView result equivalence.

TR is max(H-L, abs(H-prevClose), abs(L-prevClose)); first TR is H-L. Wilder ATR20 starts with the mean of the first 20 available TRs. Candles are classified into eight mutually exclusive classes: bullish/bearish times TR/ATR below 0.5, 1.0, 1.5 or above. Doji counts as bullish as in the supplied Pine.

PAE = Shannon entropy of 20 classes / 3. EG = (PAE[t]-PAE[t-3])/3. EPI = abs(EG) * current tick volume / mean tick volume20 (zero mean gives zero). DBD = (bullish-bearish)/10. Entry requires PAE <= 0.6 * maximum PAE50, EG < 0 for three readings, EPI > 0.70 and abs(DBD) > 0.20; DBD gives direction. The first 100 bars warm up; gaps among the latest 30 M1 bars withdraw signals.

SL distance = 0.5 * mean TR3, rounded outward to a tick; TP distance = 1.5 * mean TR3, anchored to simulated entry. Exit at the next open when the preceding closed bar's EG turns positive, or after eight minutes; gap/SL/TP already crossed at that open take precedence. This differs from Pine's `process_orders_on_close`. Meta retains its 1.5 ATR stop, target twice rounded stop distance and 30-minute exit.

Tests cover a hand-constructed CES pressure signal against independent numeric reference values, future-data independence, threshold boundaries, no-volume behavior, actual next-open entry, one commission, smart exit and eight-minute exit. These are software checks on synthetic data, not evidence of profitability.

Official MT5 reference: https://www.mql5.com/en/docs/python_metatrader5/mt5copyratesrange_py
