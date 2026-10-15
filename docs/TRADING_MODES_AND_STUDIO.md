# Trading modes and learning studio — 0.5.0

All modes drive **Shadow simulation**, not broker orders. The MT5 gateway remains read-only. Generated strategies and Wyckoff learning are research outputs without automatic promotion.

| Mode | Completed inputs and trigger | Stop / target | Maximum hold |
|---|---|---|---|
| Expert | Original Meta M1/M5/M15 consensus | 1.5 ATR / 2R | 30 minutes |
| Scalping | M1 trend + three-bar momentum, M5 confirmation | 1.2 ATR / 1.4R | 15 minutes |
| Swing | H1 momentum/trend, H4 and D1 confirmation | 2 ATR / 2.5R | 3 days |
| Sniper | M1 six-bar breakout, M5 confirmation, new quote ≤1 second | User point distances; net target includes fees | 2 minutes |

Switch modes while STOPPED. An open position retains its original stop, target, commission, slippage and maximum holding duration across changes/restarts. Existing checkpoints default to Expert's original duration. Account identity, one-position rule, broker positions/orders, risk budgets, daily/drawdown locks and lot limits still apply. STOP prevents new entries while virtual exits continue.

## Commission and spread

Commission is **round-trip per 1.00 lot, in account currency**: 7 × 0.1 = 0.70. If a broker quotes one side, enter entry plus exit. Risk and mode pages share the same setting. The calculator uses fresh MT5 bid/ask and broker point/tick specifications; it shows spread points/pips/money, commission, assumed two-sided slippage, total cost and approximate bid-to-bid break-even. Offline it calculates commission alone and marks spread/total unavailable. Display is labeled as the last manual calculation.

Pip size is explicitly configurable in points (default 10); metals and indices do not have a universal pip convention. Asymmetric tick values use the larger value for cost estimates and the profit tick value for break-even. Spread is paid through executable bid/ask, never subtracted twice from PnL. Sniper targets compensate commission and exit slippage once. Its cost gate is `cost / (cost + desired net target money)`; default maximum 0.45. The stop must exceed entry spread/slippage. Default entry cooldown is 30 seconds. No re-entry on the same exit quote.

Sniper requests 250ms **sleep between full gateway polling cycles**, versus one second normally. Account/history reads, IPC and processing add latency. This is not tick-complete/high-frequency transport, and it does not guarantee execution speed or profitable quick trades. Overnight swaps, nonlinear/minimum commission tiers and future contract changes are not modeled. Gaps can exceed planned loss.

## All native MT5 timeframes

M1/M2/M3/M4/M5/M6/M10/M12/M15/M20/M30/H1/H2/H3/H4/H6/H8/H12/D1/W1/MN1 are read from actual broker history. One bounded frame is requested every three seconds; a first rotation takes at least 63 seconds. Each request takes at most 601 bars including current, removes that unfinished bar and uses its actual opening timestamp as closure boundary, including calendar months. Core validates account, contract, order and capture/closure timestamps. Missing history is explicit; no synthetic bars replace it. Swing waits for actual fresh H1/H4/D1 frames.

## Strategy generation and learning

For each eligible frame, the studio synthesizes **40 deterministic rule compositions** from breakout, rejection, trend-pullback and Wyckoff-test families, lookbacks, ATR stops and reward ratios. This is combinatorial rule generation and statistical selection, not a neural model. At least 400 actual bars are required; up to 600 are frozen with the contract and costs.

The chronological split is 60% train, 20% validation and 20% final test, with a 24-bar purge before validation and test. Maximum holding horizon is 24 bars. Training ranks candidates; eight reach validation; the validation winner is frozen before the final test. Ranking is net R minus 0.75 × drawdown R, requiring four trades for a finite score. Test results never choose the winner. Review eligibility additionally requires ten final-test trades and positive validation/test net R; this is descriptive evidence, not a statistical proof of profitability.

The simulator enters on next-bar open, uses bid OHLC with historical spread to reconstruct executable sides, includes commission/slippage, handles stop gaps conservatively and prioritizes SL when SL and TP share a bar. Results normalize each trade to initial one-lot risk. It uses current fixed broker tick values, not historical currency conversions. Swaps are excluded. Immutable reports save selected rules, trials, split boundaries, metrics, exact dataset and assumptions in SQLite. Fees are frozen when a job is queued. Jobs run off the IPC thread and are cancellable.

Automatic learning waits until a new final-test interval starts after the latest exposed test end for the account/symbol/frame. Repeated Run clicks cannot recycle an exposed holdout. Old observed data may become later training data; each new final test remains chronologically unseen. Reports do not automatically alter Shadow rules or send orders.

## Wyckoff analysis and prospective learning

The detector estimates a prior 68-bar range, excluding the seven recent event/test bars, and requires repeated boundary touches and meaningful range width. It recognizes Spring + Test, Upthrust + Test, SOS → LPS and SOW → LPSY. Only a subsequent lower-activity test confirms direction. High effort with limited price result is displayed as possible absorption. B/C/D phase labels remain tentative; the implementation does not claim comprehensive A–E phase recognition.

Broker real volume is used only when positive across the relevant window; otherwise the interface explicitly labels tick activity as a proxy. The first current confirmed signal is saved once per account/frame/bar. Entry uses the current executable quote plus slip and frozen costs/contract. The outcome matures after **12 later completed bars**, using historical exit side/spread and fees. Horizons missed outside retained history expire instead of receiving fabricated labels. Pending/scored counts, wins, mean net per lot and a Laplace-smoothed win frequency persist after restart. A descriptive positive-evidence flag requires 20 scored observations and positive mean; it is not calibrated confidence or execution permission. Historical backfill is never presented as prospective learning.

## Persistence and APIs

A backwards-compatible defaulted `Config.trading` object preserves existing working configs. Backup schema 11 includes prior state plus studio frame snapshots, immutable report datasets, settings, forward Wyckoff observations and mode decisions. Existing working databases gain tables automatically; sealed restore still requires exact app version/schema. The existing research export now includes `mode_decisions.jsonl`.

- `/v1/trading/settings`, `/v1/trading/preview`: validated profile/commission settings and current quote cost calculation.
- `/v1/gateway/research/request`, `/frame`: bounded account-bound frame reads.
- `/v1/studio`, `/run`, `/auto`, `/cancel`, `/report`: research status and persisted reports.

References: [MT5 historical bars](https://www.mql5.com/en/docs/python_metatrader5/mt5copyratesfrompos_py), [MT5 timeframes](https://www.mql5.com/en/docs/python_metatrader5/mt5copyratesfrom_py), [Wyckoff Analytics](https://www.wyckoffanalytics.com/wyckoff-method/), [Wyckoff SMI tests](https://wyckoffsmi.com/tests-of-up-thrusts/).
