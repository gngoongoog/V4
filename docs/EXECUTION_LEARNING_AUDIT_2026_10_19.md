# BrainTrade OS V4 — Execution/learning integration audit checkpoint

Date: 2026-10-19  
Scope: source review of development after PR #64, with fixes proposed as a separate PR. This is **not** a certification that every third-party broker/terminal scenario has been tested.

## Integration pathways reviewed

| Component | Read paths | Scope verified by review and existing automated tests |
| --- | --- | --- |
| Brain registry & modes | `crates/domain/src/execution_legacy.rs`, `crates/domain/src/trading.rs`, `crates/domain/src/execution.rs` | 16 magic identities, coordinator/Advisor/SOLO membership, four trading modes, Demo-only flexible selection |
| Learning & remembered mistakes | `crates/runtime/src/coordinator_learning.rs`, `crates/runtime/src/student_learning.rs`, `crates/runtime/src/student_timing.rs`, `crates/domain/src/student.rs` | Exact closed Demo broker labels separated from quote-only research, account/context isolation, loss streaks, chronological exit/entry comparisons |
| Durable broker path | `crates/runtime/src/execution_legacy.rs`, `gateway/execution.py`, `gateway/mt5_gateway.py` | Re-check identity/authorization after durable SQLite journal, uncertain sends not blindly repeated, broker volume/stop/fill constraints |
| Exposure limits | `crates/runtime/src/execution.rs` | Aggregated open-position/queued-intent risk, pending-command accounting, portfolio settings mutation, break-even/trailing-stop risk |
| QA and delivery | `scripts/check_execution.py`, `scripts/check_flexible_demo.py`, `tests/test_broker_execution.py` and GitHub Actions | Simulated Core/Broker integration, rebuild and Windows package tests; not native MT5 Demo acceptance |

## Defects confirmed and corrected in this change

### A1 — Trailing stop incorrectly classified as *unknown risk*

Prior code returned `None` when a BUY stop was at/above entry or SELL stop was at/below entry. With Portfolio Exposure v1 enabled this set `risk_known=false` and could prevent unrelated, otherwise valid new entries. The correct **entry-to-stop loss exposure** is zero price downside in such cases, plus estimated commission. Missing, non-positive or non-finite stop values continue to fail closed. Added BUY/Sell/break-even/NaN and enabled-portfolio regression tests.

**Boundary:** A locked-in stop is not guaranteed to fill at that level during gaps or volatility. This is an *estimate* for the existing stop-risk model, not a claim of zero real-world execution risk.

### A2 — Portfolio policy was editable with unresolved broker commands

The previously stopped-only settings path did not check `pending()`, allowing changes while an open or close command remained queued/dispatched/unknown/placed/filled/partial. It also mutated memory before SQLite persistence. The settings path now rejects all such changes until reconciliation, commits the new policy before switching its in-memory copy, and preserves pending broker-state journal/authorizations. Added all six pending-state, rejection-state, reload-persistence tests.

### A3 — MT5 DONE acknowledgement without a deal ticket was treated as a completed fill

Some brokers acknowledge an order with `TRADE_RETCODE_DONE`/`DONE_PARTIAL` but no deal ticket in the immediate response. Prior code marked this `filled`/`partial`, but the learning ledger requires an exact opened deal. The gateway now records such results as **placed / awaiting evidence**, keeps the durable send journal and attempts exact history reconciliation. When a successful response contains an order number, history can be matched by **exact order ticket + magic + symbol + account**, even if a broker edits the comment. If a returned order ticket conflicts with history, an identical-looking comment is not enough to attribute the fill. The order is **never resent blindly**.

Regression tests cover missing deal IDs for DONE/DONE_PARTIAL, restart persistence, recovery with changed broker comments, disjoint broker orders and misleading matching comments. Remaining limitation: if a broker provides neither usable history order IDs nor intact comments, Core cannot safely infer the fill and requires manual reconciliation.

## Existing controls checked but intentionally not weakened

- No unconditional/random trades or bypass of broker minimum lot/margin, executable bid/ask, quote timestamp freshness, account identity, Core authorization, STOP semantics or append-only magic ownership.
- Account-isolated Demo learning must not modify Live execution logic.
- Research/counterfactual outcomes must never masquerade as booked broker profit.
- A stale or uncertain send must be reconciled rather than repeated just to increase apparent trade count.
- Automatic exit selection stays Demo-only with validation and rollback.

## Pending acceptance work: cannot be proved by GitHub CI

1. Native Windows + the intended **MT5 Demo broker**: successful fills/rejections, partial closes, position ownership, exact round-trip costs and logged execution latency.
2. Restart with uncleared pending/unknown intents; compare journal and broker history for duplicates.
3. Sniper MT5 OnTick against Poll over separate sessions; inspect P50/P95/P99 latency before optimizing speed.
4. Test $100 and other small accounts with **actual symbol volume/margin constraints**, including broker minimum-lot rejection.
5. Accumulate forward OOS records across different spreads, trading sessions, news windows and regime shifts. Verify learning improves out-of-sample *after costs*; simulated CI does not establish this.

## Verification policy

Use the exact PR-head Research and Windows/Linux Actions runs. Merge only when both succeed. Follow with post-merge verification on `development`, preserving `main` promoted 0.6.0. No production-readiness or profitability guarantee is made by this audit.
