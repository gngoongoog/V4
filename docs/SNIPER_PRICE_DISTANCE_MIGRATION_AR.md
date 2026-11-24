# Sniper price-distance migration — development branch

Branch: `feature/sniper-price-distance-fill-20261124`

## User-facing contract
Sniper TP and SL are price movements in the symbol's quoted price, not pips,
points, or account-currency profit. Example: for a confirmed XAUUSD fill of
4098.123, BUY TP distance 0.25 and SL distance 1.00 produce TP 4098.373 and
SL 4097.123 (subject to exact broker tick-grid and stops/freeze validation).
SELL mirrors the direction.

## Safety invariants
- MT5 contract metadata remains authoritative: point, tick size, stops level,
  freeze level, volume min/step/max, and tick values.
- Never silently round a requested distance upward or widen a stop/target.
- A distance that cannot be represented exactly on the broker tick grid is
  rejected with an actionable explanation.
- Risk sizing continues to use MT5's broker profit calculator and actual SL.
- A confirmed fill must be sourced from exact MT5 deal/position evidence.
- No order may be left intentionally unprotected. The current Gateway sends
  attached SL/TP using pre-send quote geometry; replacing this with
  fill-relative protection requires a separately journaled, idempotent
  post-fill SLTP modification plus a tested fail-closed emergency policy.
- Do not enable this path for live trading until post-fill protection and
  reconciliation are integrated and tested end-to-end.

## Current implementation stage
This branch introduces and unit-tests a pure fill-relative price geometry
function. It is not yet wired into the order-send path or UI. This is an
intentional safety boundary: a helper alone does not make actual-fill
protection production-ready.
