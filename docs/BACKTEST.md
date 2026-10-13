# Historical research — 0.3.0

The native Backtest page supports Meta v1 and CES v1. Live quote-sampled Shadow
continues to use Meta. Historical testing never submits broker orders.

## Run and replay

Choose UTC start/end dates, one to 30 calendar days; the end date is exclusive.
Choose virtual equity in the account currency and a spread floor in price units.
The example 0.20 is for a gold-like symbol and must be changed for other markets.
Risk settings are frozen when starting the job. Press STOP and let the current
virtual position close before fetching history, so history reads do not interrupt
an active simulation. Current MT5 broker positions are read-only and unaffected.

The Python gateway requests completed M1 bars using timezone-aware UTC dates and
`copy_rates_range`, one day per cycle, including seven calendar days of warmup.
The MT5 terminal must have the history available; its Max bars setting and broker
coverage can limit results. Empty periods are not fabricated. Partial coverage
and time gaps appear in each report. Account, server, symbol, currency and point
must match the frozen request; identity changes reject the chunk. Cancelling
rejects late chunks without damaging a newer job.

The core worker runs independently of the UI. Reports and exact input bars are
stored together in SQLite before a job is marked completed. A saved run can be
opened, exported or replayed offline after restart. Replay freezes the original
strategy, equity, dates, costs and risk. Experiment uses the selected strategy
on the selected saved run's exact data and assumptions, creating a separate run;
editing the form's equity/spread does not change an offline experiment. This is
not walk-forward testing or a parameter optimizer.

Exports include report.json, history.json, trades.csv, equity.csv, report.html
and a README. The HTML is an escaped, self-contained local document with an SVG
equity curve and up to 200 trade rows; CSV/JSON contain all trades. Opening it is
an explicit dashboard action. Saved data and reports are included in backups.

## Common fill model

- Signal features use only candles closed before the entry minute. Meta needs
  complete M5/M15 groups aggregated from M1; no missing bars are interpolated.
- BUY enters ask plus configured slippage; SELL enters bid minus slippage.
  Ask is approximated by a constant spread for each minute: the larger of the
  user floor and the PREVIOUS completed M1 spread times symbol point.
- BUY exits use bid; SELL exits use ask. Slippage is charged at both fills and
  configured round-trip commission once. Minimum lot is never rounded upward
  through the risk budget; broker step and maximum lot are respected.
- One virtual position at a time; no re-entry during its exit minute. If both
  SL and TP lie inside an OHLC range, SL wins and the trade is marked ambiguous.
  A stop crossed at the open fills at that worse open. A target gap fills at the
  target. Missing-minute gaps close an existing position at the first observed
  open with a data_gap flag; no intermediate fill is invented.
- Daily loss resets on Baghdad day boundaries; maximum drawdown latches for the
  run. Equity is marked at open, close and fills, not every intrabar excursion.
- Current tick values/contract specifications are frozen. Historical currency
  conversion, margin, swaps and broker execution are not modeled. Zero-trade
  runs and undefined profit factors are stated explicitly, not treated as wins.

## CES default implementation

Source: the user's `CES_v1_Critical_Entropy_Scalper.pine`, read from its current
saved contents. That file defines EPI using VOLUME, despite a candle-size variant
in earlier discussions. This port follows the file and substitutes MT5 tick
volume, not exchange traded volume. No claim of TradingView result equivalence.

TR is max(H-L, abs(H-prevClose), abs(L-prevClose)); first TR is H-L. Wilder ATR20
starts with the mean of the first 20 available TRs. Candles are classified into
eight mutually exclusive classes: bullish/bearish times TR/ATR below 0.5, 1.0,
1.5 or above. Doji counts as bullish as in the supplied Pine.

PAE = Shannon entropy of 20 classes / 3. EG = (PAE[t]-PAE[t-3])/3.
EPI = abs(EG) * current tick volume / mean tick volume20 (zero mean gives zero).
DBD = (bullish-bearish)/10. Entry requires PAE <= 0.6 * maximum PAE50, EG < 0
for three readings, EPI > 0.70 and abs(DBD) > 0.20; DBD gives direction.
The first 100 bars warm up; gaps among the latest 30 M1 bars withdraw signals.

SL distance = 0.5 * mean TR3, rounded outward to a tick; TP distance = 1.5 *
mean TR3, anchored to simulated entry. Exit at the next open when the preceding
closed bar's EG turns positive, or after eight minutes; gap/SL/TP already crossed
at that open take precedence. This differs from Pine's process_orders_on_close.
Meta retains its 1.5 ATR stop, target twice rounded stop distance and 30-minute exit.

Tests cover a hand-constructed CES pressure signal against independent numeric
reference values, future-data independence, threshold boundaries, no-volume
behavior, actual next-open entry, one commission, smart exit and eight-minute exit.
These are software checks on synthetic data, not evidence of profitability.

Official MT5 reference: https://www.mql5.com/en/docs/python_metatrader5/mt5copyratesrange_py
