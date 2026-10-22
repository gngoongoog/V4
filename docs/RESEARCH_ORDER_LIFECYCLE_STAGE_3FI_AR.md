# Stage 3F-I — Independent synthetic broker order-lifecycle replay

## Scope

This is a **self-contained research endpoint** `POST /v1/research/execution-lifecycle` on the existing authenticated local Core server. It takes ONLY an explicit JSON scenario; it never reads the selected live or Demo MT5 account, dispatches an order, mutates the trading or learning systems, or writes a broker deal. The returned synthetic ledger contains no claimed booked or unrealized profit. It is a testable foundation for future independent selected-coordinator execution simulation, **not yet a full execution twin**.

The replay accepts a bounded chronological list of explicit events:
- `submit`: one unique order identity and requested lot volume;
- `dispatch`: a hypothetical request has been sent; not evidence of a fill;
- `unknown`: the simulated send outcome cannot be determined; new fills are **never invented** and unsent cancellation is forbidden;
- `fill` with a globally unique deal ID and *incremental*, positive volume, which may partially fill; duplicate matching deal IDs replay idempotently;
- `close` with a distinct deal ID and volume no larger than already filled, still open simulated volume;
- `reject_remaining`: explicit rejection of the unfilled remainder; already filled/open volume is preserved;
- `cancel_unsent`: only for submitted orders that were never dispatched.

All events are processed with strict transition and volume validation. Invalid scenarios return an error, not a partial success ledger. Account and symbol are mandatory synthetic labels; the whole scenario is scoped to one synthetic account/symbol. All open and unresolved volume is reported separately. No timestamp, price, margin, commission or broker PnL is implied.

## Example input

```json
{
  "account": "demo_77",
  "symbol": "XAUUSD",
  "events": [
    {"event": "submit", "id": "entry_1", "lots": 0.10},
    {"event": "dispatch", "id": "entry_1"},
    {"event": "unknown", "id": "entry_1"},
    {"event": "fill", "id": "entry_1", "deal_id": "deal_101", "lots": 0.04},
    {"event": "reject_remaining", "id": "entry_1"},
    {"event": "close", "id": "entry_1", "deal_id": "deal_102", "lots": 0.04}
  ]
}
```

The expected final order state is `partial_terminal_flat` with zero open volume and zero unresolved dispatched entries. The result explicitly declares `no_mt5_commands=true` and `profit_loss=null`.

## Contract and verification

Input is capped at 1 MB / 5000 events; IDs are bounded, exact deal identities are globally unique, invalid/nonfinite lot amounts fail. Rust synthetic tests cover partial fills, unknown sends, duplicate deal replay, overfill, overclose, cancel-after-send denial and explicit remainder rejection. Execute exact-head Linux/Research/Windows checks before merging.

## NOT supplied by this stage

No genuine broker account margin, leverage, volume step rounding, bid/ask prices, stop/take-profit outcomes, slippage, latency, order-book depth, partial financial settlement, account-wide portfolio exposure, multi-symbol correlation, position reconciliation from MT5 history or MT5 acceptance evidence. This is a *state-machine primitive* to be wired into a later real-price **research-only** execution simulator, not a broker gateway or a trading recommendation.
