# Stage 3F-G — Prospective paired quote-paper outcomes for a frozen Expert coordinator

**Scope: MT5 Demo, selected Expert mode only, strictly no MT5 broker orders or changes to learning or risk gates.**

## Why this exists
3F-F counts frozen-versus-adaptive decision agreement on newly completed M1 bars, which cannot establish whether adaptation improves trade outcomes. This stage compares a *restricted both-ready cohort* using the SAME fresh observed Demo Bid/Ask at entry, 0.01 lots per hypothetical arm, the same captured full-roundtrip per-lot commission and assumed two-sided slippage. Each arm keeps its own entry-time side, stop distance, target distance, and maximum holding time. Subsequent real-time sampled quotes trigger independent stop, target, or time exits and the two hypothetical legs must BOTH close before a pair counts as completed.

## Controls and durability
- Frozen snapshots and sampled decision pairs remain immutable, account/symbol scoped and preserved after STOP.
- New entries require an active frozen coordinator policy, a *fresh same-cycle* M1 decision record and both signals ready with valid distances.
- Only one open pair per frozen policy at a time. Already open simulated positions continue settling after policy stop, mode changes or application restart when fresh Demo quotes arrive.
- A delayed observation, older decision row, invalid/stale quote, or no matching broker account cannot fabricate a new pair or profits.
- All results are in `frozen_coordinator_trade_pairs`, separate from order journal, MT5 reported deals, optimizer rewards and adaptive learner evidence. The system row on `حالة العقول والتحكم` shows completed/open pairs, quote gaps, winning pairs and separate mean hypothetical net-per-lot for each policy.
- Pricing assumes a flat 0.01 lot with the frozen entry-time broker tick values and the configured assumed roundtrip commission. When configured commission is $7 per lot, each arm pays $0.07 per completed hypothetical trade; 0 disables assumed commission.

## Not a complete institutional execution twin
This is a **research-only, sampled-price Paper comparison**. It does not reproduce exact broker order placement, risk gates, margin checks, portfolio/position gates, min-volume adjustments, real execution spread/slippage, partial fills, funding or commissions booked by the broker, netting/hedging side effects, command retries or shutdown reconciliation. Stop/target crossings between unobserved quotes may occur at very different real fill prices. Identical quote-time entry is an *assumption*, not proof that two real strategies could fill identically. The comparison includes only moments BOTH policies were actionable and excludes missed and overlapping opportunities; this is selection bias. It cannot support a profitability or REAL readiness claim.

## Verification
Rust unit tests cover independent exit geometry, costs, stale quotes, invalid decision bars, only one overlapping pair, STOP settlement, differing signal timeframes sharing one M1 decision record and rejection of late-sourced decisions. Research/Linux and native Windows checks must pass at the exact PR head. Native MT5 Demo quote continuity/fees, restart and account isolation still require operator acceptance on the target Windows machine.

**Next stage** should be a genuinely independent whole-coordinator simulator with broker/order states, margin/portfolio gates and side-by-side broker acceptance observations before institutional REAL approval could even be considered.
