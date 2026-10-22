# Stage 3F-D — fair same-quote five-minute forward directional markouts

## Goal
Enable a **matched directional evaluation** for the *frozen non-system Expert advisor* and the *currently learning advisor* running simultaneously on the same MT5 Demo symbol. This is **read-only and paper-only**; it cannot change broker order flow, training weights, risk controls or promotion rules.

## Cohort and data provenance
A trial starts only through the user's Demo freeze consent from 3F-C. A matched observation is inserted **once per run and completed signal bar** if both advisors have `ready=true` and a valid `side` ±1, their `bar_time` is exactly equal and strictly post-freeze, the quote is a fresh MT5 Demo Bid/Ask snapshot, and no other matched observation is pending for that run. This prevents comparing different calendar windows or retroactively selected price paths.

Both arms enter at the same sampled quote: long buys the Ask, short sells the Bid, with the same **fixed 0.01 lots**, the same **frozen** tick size, tick values, commission-per-lot and two-sided slippage assumptions. This makes their costs comparable even if the continuously learning advisor subsequently modifies its normal lot and target policy.

A single future quote no earlier than **300 seconds after entry**, and no later than **30 seconds after this target**, settles both arms on Bid or Ask by direction. If no sufficiently timely fresh quote exists, the pair is marked **expired** with **no imputed return**. No subsequent quote is cherry-picked as a substitute.

Only one non-overlapping matched opportunity may be outstanding per run. Repeated polling, restarts, and re-evaluation of the same signal bar do not double-credit the cohort. Retiring a trial suppresses new matches but still settles or expires an already open match.

## Metrics
- Number of settled opportunities, pending opportunities, expiries (missing timely quote).
- Mean net simulated result **per lot** for each arm, and mean signed difference (frozen minus adaptive).
- Count where frozen markout is higher, and exact ties.
- Until there are at least 30 valid completed pairs, status is **insufficient_matched_evidence**. Afterwards still only **descriptive_only**, not statistical superiority.

The markout uses a **fixed five-minute horizon with no interim stop loss or take profit**; it is not the strategy's execution PnL. Its figures do **not** model broker fills, delayed/partial orders, swap/overnight costs or price paths between sampled quotes. Inclusion requires both candidates ready, creating a selective cohort; even >30 observations cannot alone establish alpha. Tick values and slippage are held constant from the opening observation; brokers may differ.

## Operational isolation
The module writes only to `frozen_shadow_markouts` and reads the same persisted frozen snapshot from 3F-C. `Core` continues gating the actual MT5 path independently. Quote, accounting, or SQLite errors in the trial are isolated and logged by the pre-existing Shadow boundary; no failure in this study may force broker execution errors or convert Paper outcomes to Demo fills.

## Test plan
Rust unit tests cover cost symmetry (including full $7/lot commission), opposite directions, missing settlement quotes, strict same-bar matching, stale quotes, idempotency, and absence of broker orders. CI must pass exactly matching commits in Research + Linux + Windows before merging this stage into main; manual MT5 Demo validation is still needed.
