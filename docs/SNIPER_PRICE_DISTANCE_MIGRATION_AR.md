# Sniper price-distance migration — development branch

Branch: `feature/sniper-price-distance-fill-20261124`

## User-facing contract
Sniper TP and SL are price movements in the symbol's quoted price, not pips,
points, or account-currency profit. Example: for a confirmed XAUUSD fill of
4098.123, BUY TP distance 0.25 and SL distance 1.00 produce TP 4098.373 and
SL 4097.123 (subject to exact broker tick-grid and stops/freeze validation).
SELL mirrors the direction. The native UI and domain defaults are aligned at
TP distance 0.50 and SL distance 0.80; users can change both while stopped.

## Safety invariants
- MT5 contract metadata remains authoritative: point, tick size, stops level,
  freeze level, volume min/step/max, and tick values.
- Never silently round a requested distance upward or widen a stop/target.
- A distance that cannot be represented exactly on the broker tick grid is
  rejected with an actionable explanation.
- Risk sizing continues to use MT5's broker profit calculator and actual SL.
- Every Sniper market request first carries broker-validated quote-based SL/TP.
  A filled order is then linked to its exact opening Deal, order ticket, and
  physical MT5 position before any post-fill modification is attempted.
- The Gateway computes levels from the observed position average fill, checks
  the current quote/stops/freeze constraints, and runs broker preflight.
- The SLTP modification has its own durable `sends` journal committed before
  the MT5 write. The Gateway revalidates the exact account and position, sends
  the modification once, and verifies the resulting SL/TP on that same position.
- If the modification result is uncertain, reconciliation observes the exact
  position and journal; it never blindly repeats the SLTP write.
- If Deal/position linkage or modification cannot be proven, the original
  attached quote-based protection is retained, the command becomes UNKNOWN,
  and the entry is not blindly retried. This is not a claim that the requested
  fill-relative geometry was applied; operator/reconciliation review is needed.
- Partial fills are not re-anchored while the broker order may still have a
  remainder. Their initial attached protection remains; they require terminal
  order reconciliation before a future safe enhancement can re-anchor them.
- A broker stop/freeze restriction never causes silent widening. A requested
  distance that cannot be represented exactly is rejected with an explanation.

## Current implementation stage
Fixed and adaptive Sniper strategy geometry, UI inputs, cost gates, learning
identity, and gateway protection now use quoted-price distances. Full fills
have a tested post-fill SLTP re-anchor path. This branch is still NOT production
approved: automated tests and native Windows/MT5 runtime have not yet been
executed in this environment. Do not use it for Live trading until CI, real
broker Demo validation, and partial-fill reconciliation are completed.
