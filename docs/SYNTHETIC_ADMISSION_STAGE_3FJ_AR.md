# Stage 3F-J — Synthetic fail-closed entry admission for portfolio research

## Design boundary

A standalone authenticated local Core endpoint `POST /v1/research/execution-admission` consumes a bounded JSON **synthetic scenario** and returns deterministic **hypothetical eligibility**, never a broker authorization. It does not touch live/demonstration MT5 account state, cannot place or cancel orders, and does not write broker journals or train weights. It follows the detached lifecycle state machine in Stage 3F-I; the two separate research modules are not yet a complete broker twin.

Scenario inputs: synthetic account and symbol identity, fresh bid and ask/quote age, volume min/max/step, tick size and loss tick value, an optional estimated margin-per-lot, equity/free margin, limits on position slots, total lots and maximum stop risk as percent of equity, round-trip commission per lot and slippage ticks per side. Optional daily loss breaker has a separate *synthetic* day-start equity baseline; the baseline model does not account for deposits, withdrawals or broker trading history and must not be treated as actual daily loss. Existing synthetic exposure supplies volumes and **known** or `null` stop-risk cash. Requested entry order includes side, lots and absolute stop price.

## What is checked

For each candidate in the supplied order, with no state carried between requests:
- Quote must not be older than 3 seconds; broker volume min/max/step must align **without silently rounding lots up**.
- BUY enters at ask and requires stop strictly lower. SELL enters at bid and requires stop strictly higher. Stop loss ticks, supplied loss tick value, assumed two-sided slippage and full roundtrip commission contribute to cash-at-stop estimate.
- Current hypothetical holdings plus previous *eligible hypothetical reservations* cannot exceed position slots, total lots, cash stop-risk budget or the externally supplied **linear** margin estimate against free margin.
- Missing stop risk for any existing position, or missing margin estimate, means the relevant candidate is blocked; **unknown is not pass**.
- With synthetic daily loss breaker enabled, missing synthetic day-start equity or daily equity drawdown above threshold blocks all new entries. Disabling it in a scenario is a research-only input, not a request to disable a real account safety feature.
- Every request has a distinct identity; malformed scenarios fail atomically rather than producing a partial admission ledger.

Only accepted *hypothetical* candidates reserve simulated capacity for later candidates. Returned output explicitly says `broker_approvals=0` and `mt5_commands=0`.

## Example

```json
{
  "account":"demo_77","symbol":"XAUUSD",
  "quote":{"bid":5000.0,"ask":5000.2,"age_ms":50},
  "contract":{"volume_min":0.01,"volume_max":10.0,"volume_step":0.01,
    "tick_size":0.1,"tick_value_loss":1.0,"estimated_margin_per_lot":250.0},
  "limits":{"equity":500,"free_margin":300,"max_positions":2,
    "max_total_lots":0.1,"max_stop_risk_percent":2,
    "commission_roundtrip_per_lot":7,"slippage_ticks_per_side":1},
  "existing":[],
  "requests":[{"id":"one","side":"buy","lots":0.01,"stop_price":4999.2}]
}
```

For the stated synthetic market terms, the per-order stop-risk estimate is **$0.19**, including $0.07 configured roundtrip commission at 0.01 lot. Real MT5 tick value, contract, spread, commission and minimum volume may differ. This is a deterministic arithmetic test, not a PnL forecast.

## Remaining institutional gaps

The model is single-symbol, uses a simplified stop cash estimate and **caller-supplied** linear margin. It does not call MT5 `order_check`, `order_calc_profit` or `order_calc_margin`, model broker stop-distance limits, hedging/netting, multi-symbol positions, currency conversions, real partial fills, preemption, order deduplication between restarts, market impact or quote replay. Risk and daily loss figures are not authoritative. The next stage must integrate independently verified Demo broker outcomes and realistic multi-symbol portfolio state for meaningful execution parity.

Run exact-head Rust/Research/Linux/Windows checks before merging. Even a green CI does **not** certify trading profitability or production readiness.
