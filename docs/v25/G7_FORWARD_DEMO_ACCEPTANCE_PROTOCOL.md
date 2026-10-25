# BRAINTRADE V25 — G7 | Forward MT5 Demo acceptance protocol (NOT an acceptance result)

Date: 2026-10-26
Starting source: `ac3302be5d2378b70869c62d09463f14d175a1db` (G6-L).
Scope: prospective evidence and operational acceptance only. **No Live authorization, order/lot/risk changes, main merge, or claims of profitability.**

## Why this gate exists
G6-F..L strengthen local admission causality, sample integrity and deterministic Student-exit-study replay. Passing Rust, Research, Windows or Portable CI establishes software behavior **in test fixtures**, not quality of future market decisions, real broker fills, or adaptive superiority. A local SHA-256 and SQLite arrival timestamp are not an independently trusted external timestamp.

## Preregistration before data collection
Record an immutable, human-reviewable manifest containing:
- Source commit; Windows binary SHA-256; gateway and EA version/hash; full configuration snapshot, account type and broker/server identifiers (redact credentials); symbol contract specifications, server timezone and testing start/end.
- Fixed hypothesis, primary outcome, minimum sample target, planned observation horizon, max allowed missingness, abort conditions and analysis script version.
- Explicit group labels `Frozen` and `Adaptive`, their exact weights/policy checkpoints at the experiment cut-off, frozen decision rules and the deterministic allocation scheme **defined before the first eligible decision**.
- Pre-existing decisions, outcomes and aggregates stay in historical/research partitions. Do not backfill the prospective cohort.
- Research result and manifest must both say `not_started` until a real future observation is present. “Number of samples” alone never means learned edge.

## Prospective decision records
For every eligible decision, preserve the same *pre-decision* market snapshot for both policy recommendations before any future outcome can enter either policy. Record account, physical position/command IDs, mode/brain/timeframe/report, pricing clock, source admission clock, chosen policy, policy checkpoint hash, quote IDs, spread, volume, signal, predicted geometry, and whether an order was actually sent.
Distinguish:
1. `counterfactual_signal`: comparison using saved pre-decision quotes only. **Not** a booked P&L/fill.
2. `mt5_demo_execution`: broker order, fill, rejection, close, fees, swap, slippage and actual realised R, reconciled from MT5 history.
3. `excluded`: late arrival, wrong account/symbol, missing admission/price/costs, duplicated close/physical position, stale quote, compromised checkpoint or incompletely closed position.

Never let a counterfactual row appear under broker P&L, trade win rate, or booked learner reward. Never present a benchmark result as though *both* arms were executed when only one was actually executed.

## Assessment rules
- Fixed denominator: report every pre-registered eligible decision, executed trade, no-fill, missing label and exclusion with exact reason; report separately by selected trading mode.
- Report net booked R, total net P&L, drawdown and drawdown horizon, execution spread/commission/swap/slippage, confidence interval / uncertainty estimate, and worst-case sensitivity for missing labels. Preserve trades with zero and negative results.
- If only Adaptive was executed, there is **no broker-executed Frozen control**; mark paired signal analysis as hypothetical, not a verified execution advantage.
- If arm assignments were not prospective and comparable, do not claim causality. Use account/broker clock reconciliation and check that neither arm saw a late label before the decision.
- Report independent future window performance, *not* reuse of the training period for promotion. Define an explicit decision threshold in the preregistration manifest; any post-hoc thresholds are exploratory.
- Treat insufficient samples, unbalanced arms, weak power or inconclusive intervals as `inconclusive`, never `pass`.

## Native Windows / broker operator acceptance
Perform on an actual Windows device, connected to a known MT5 **Demo** terminal when open markets and approved account access permit:
- Record OS, installer/artifact hash, WebView/Slint startup, and opening/navigation/saving of all 22 pages. For every actionable control, record the observed result or a reproducible error.
- Verify six-field account identity and Demo status, symbol mapping, tick timestamp/size/value, live Bid/Ask, M1/M5/M15 candles, margin information and disconnect/reconnect behavior. No fabricated offline values.
- Verify START/STOP, mode ownership, duplicate suppression, stop/recovery, partial close, reject/timeout/requote and reconciliation against broker history without bypassing risk protection.
- Run a low-risk Demo order only after identity, pricing, user controls and all safety checks pass. Compare position/order/deal IDs, fees and booked profit with MT5 history.
- Verify Telegram delivery separately; successful mock/config parsing is not external message-delivery acceptance.
- Preserve screen captures, logs, actual dates, broker statements (credentials removed), pass/fail and reproducible steps; unavailable tests must say `NOT RUN`.

## Release decision matrix
| Gate | Result at creation of this file |
| --- | --- |
| G6-L local replay implementation and automated tests | Implemented; exact-head G6-L, Research, Portable succeeded during latest observed check |
| G6-L native Windows build | In progress at latest observed check — re-check exact SHA |
| 22-page native manual Windows acceptance | NOT RUN / no independently verified evidence |
| Broker-connected MT5 Demo fills + reconciliation | NOT RUN / no independently verified evidence |
| Prospectively allocated Frozen-vs-Adaptive executed comparison | NOT RUN / no independently verified evidence |
| Independently better OOS outcome | NOT PROVEN |
| REAL readiness | NOT AUTHORIZED |

## Gate policy
Do not merge the stacked V25 release into `main`, enable REAL deployment or promote a policy on the strength of local hashing, green CI or synthetic/demo-like fixtures alone. This document is a concrete protocol, **not** completed G7 experimentation. Next implementation work should automate preregistered manifests, immutable arm assignment/checkpoint recording, strict row accounting and an outcome report with `inconclusive` as the safe default. Run true native and MT5 Demo checks on a user's machine and evidence artifacts before claiming operational acceptance.
