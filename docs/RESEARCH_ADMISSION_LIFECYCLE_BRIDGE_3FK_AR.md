# Stage 3F-K — Unified synthetic admission → order-lifecycle research bridge

## Purpose and containment

This stage composes **two existing read-only models** behind an authenticated local Core route:
`POST /v1/research/execution-bridge`. Its inputs are *only supplied fictional research scenarios*; this route does not read MT5 telemetry, submit broker commands, commit SQLite rows or change learning state.

It independently validates:
1. The complete synthetic admission scenario with the Stage 3F-J contract, portfolio, fee, margin estimate and risk-cap model. The admission output marks each requested entry `eligible_hypothetical` or denied with reasons.
2. The supplied Stage 3F-I order-event sequence: `submit`, `dispatch`, `unknown`, `fill`, `close`, `reject_remaining`, `cancel_unsent`. It never infers fills from sent/unknown statuses, and duplicates of the same broker-deal ID cannot change lots twice.
3. The bridge enforces exact **synthetic account**, **symbol**, **order ID**, and **requested lots** alignment. A lifecycle entry without a matching *eligible* admission is rejected. Dispatched and filled synthetic orders must first have an explicit submit event, following the separate state machine.

Only actual `submit` events appear in the lifecycle ledger. A hypothetical admission that was never submitted is reported as **eligible but not submitted**; it is neither counted as a sent order nor a fill. Because Stage 3F-J reserves capacity for every admitted candidate, including those not subsequently submitted, the independent admission model is intentionally conservative and may understate remaining available capacity.

## Example

```json
{
  "admission": {
    "account":"demo_77","symbol":"XAUUSD",
    "quote":{"bid":5000.0,"ask":5000.2,"age_ms":30},
    "contract":{"volume_min":0.01,"volume_max":2.0,
      "volume_step":0.01,"tick_size":0.1,"tick_value_loss":1,
      "estimated_margin_per_lot":100},
    "limits":{"equity":500,"free_margin":100,"max_positions":1,
      "max_total_lots":0.1,"max_stop_risk_percent":2,
      "commission_roundtrip_per_lot":7,"slippage_ticks_per_side":1},
    "existing":[],
    "requests":[{"id":"A","side":"buy","lots":0.04,"stop_price":4999.2}]
  },
  "lifecycle":{
    "account":"demo_77","symbol":"XAUUSD",
    "events":[{"event":"submit","id":"A","lots":0.04},
      {"event":"dispatch","id":"A"},
      {"event":"unknown","id":"A"}]
  }
}
```

The bridge must return one *hypothetically admitted and dispatched but outcome-unknown* entry with **zero broker fills inferred**, `mt5_commands: 0`, `broker_authorizations: 0` and `no_persistent_mutation: true`. It does not claim real open positions or realized trading returns.

## Validation and non-goals

Rust tests check rejected-admission blocking, account/symbol and volume mismatches, handling `unknown` send results, partial fills with duplicate IDs, and differentiating an eligible but unsubmitted request. Exact-head GitHub Actions Linux/Research/Windows checks must succeed before merging.

**Still not a full MT5 simulation:** no live quotes, `order_check`, `order_calc_profit`, broker margin engine, multi-symbol book, corporate actions, broker stop-distance rules, bid/ask tick replay, order expiration, exchange order book, live spreads, swaps, PnL, account funding flows, restart-time journal snapshots or statistical validation against historical/demo broker logs. The underlying risk figures are caller-supplied synthetic approximations. Do not use this bridge to approve a real account for live trading.
