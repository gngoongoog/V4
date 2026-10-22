# Stage 3F-N — Read-only MT5 Demo deal vs historical Bid/Ask audit

## Problem addressed

Stage 3F-L compares only synthetic volumes to caller-supplied broker deal rows. Stage 3F-M can export locally observed MT5 Demo deals without claiming historical completeness. Neither shows whether the recorded **fill prices** differ materially from contemporaneous broker Bid/Ask quote snapshots.

This stage provides a separate **explicitly launched**, read-only Python diagnostic:

`gateway/demo_historical_price_audit.py`

It runs against an already opened, connected **MT5 Demo account only**, reads historical trade deals and nearby historical ticks in the same symbol, and reports a *non-lookahead reference price* when possible. It never calls `order_send` or `order_check`, requests new trades, modifies the Core, exports account credentials or changes learning/risk parameters. It cannot run in a REAL/Contest/unverified account.

### Running on a Windows Demo workstation

Example of an arbitrary UTC historical range (milliseconds, end exclusive; not current quote data):

```powershell
python gateway/demo_historical_price_audit.py --symbol XAUUSD --from-ms 1790000000000 --to-ms 1790007200000 --max-quote-age-ms 1500 --output demo_gold_price_audit.json
```

Keep the output private; it includes account/server identifiers and historical trades. The exporter will not overwrite an existing output file. Its default max quote age is 1500 ms; accepted values are 1–3000 ms. The requested UTC interval is at most 24 hours and the history query at most 300 total deals. No automatic polling or scheduled reads.

### How the metric is calculated

For each reported MT5 deal, read `type` to establish actual broker deal side `BUY`/`SELL`; `entry` identifies whether the deal is `IN`/`OUT`. For a supported deal, select the latest valid historical tick at or **before** the deal timestamp, no older than the configured age. A tick after a deal is never used for matching.

- BUY execution expected reference = historical **Ask**; indicative adverse price deviation (ticks) = `(deal_price - historical_ask) / tick_size`.
- SELL execution expected reference = historical **Bid**; indicative adverse price deviation (ticks) = `(historical_bid - deal_price) / tick_size`.
- Positive result means *worse relative to that reference*, negative result *better*; these values do **not** prove actual broker slippage or order fill fairness.
- Report historical quoted spread `(ask - bid) / tick_size`, deal-side, price, quote staleness in milliseconds, linked MT5 ticket and position IDs and **per-deal** broker commission + fee. Profit and swap are reported separately; they are not conflated with spread/slippage.
- Unsupported deal types, INOUT/reversals and unmatched historical quote samples are explicitly **unknown**. A missing tick query (`None`) aborts the whole audit, rather than manufacturing zero slippage.

Because a historical snapshot can be sparse, delayed, sampled, or include quote updates that were not executable at the time an order was processed, a matched price still yields `quote_executability_verified=false` and `profitability_verified=false`. A prior quote is not a broker attestation and does not prove that the trade request could have succeeded. The file is a local *research sample*, not a risk permission.

### Data and evidence constraints

The audit checks the Demo account's login/server/currency/trade mode both before and after the bounded read, rejects account changes, duplicate deal tickets, impossible timestamps, malformed price/lot/fees, missing history, unavailable tick data or excessive sample sizes. `complete_history=false` always because a bounded historical query cannot prove completeness. It never guesses how a broker deal corresponds to a synthetic intent.

The independent Stage 3F-L volume parity audit remains separate; there is **no automatic merger** of synthetic reward with broker PnL or tick reference deviations. Later work is required for authoritative order ticket correlation, broker execution request/reply timestamps and retcodes, deal history reconciliation across restarts, netting and hedging, multi-symbol currency conversion and statistical out-of-sample comparison.

Fake-MT5 Python tests cover BUY/SELL price-deviation signs, fee separation, stale/future-only quote refusal, Demo-only account guard, account switch, missing history, duplicate tickets, unsupported netting deal types and the guarantee that no order is sent.
