# BRAINTRADE V4 — Enterprise V25 / P1-K5C: Learner freeze-time training provenance

**Scope:** Research-only account-scoped training cutoff for Sniper Learner; stacked on P1-K5B / PR #140. No change to the executing strategy, risk manager, MT5 sends, broker journal or REAL permissions.

## The gap closed
P1-K5B offers a five-second future Midquote directional observation, but before this phase it did not prove that the frozen in-memory Learner scores still represented the same completed Demo broker ledger at operator freeze time.

At a STOPPED operator freeze, Rust Core now reads **up to the latest 2,000 rows** of the deduplicated `coordinator_broker_samples` for the exact current Demo account and `sniper_learner`. The replay matches the same filters used by the Learner's model refresh: the last 30 days, closed before the cutoff, `mt5_demo_broker`, Sniper mode and policy identity, finite realized net/reward, valid policy/arm. It compares the count of accepted strategy and target-arm samples against the *in-memory engine scores* to reject stale snapshots. Any future-dated broker close rejects the freeze.

There is no SQLite read or write on a Fast Pulse Tick. The provenance snapshot is held inside the frozen research trial, alongside the 5-second direction label counters, and is exposed via the account-scoped existing `/v1/execution/frozen-sniper` report. The Windows Sniper page displays the source and cutoff times only when the bound Demo account and schema match.

## What this evidence does and does not prove
- Proves that **sample counts** used by the frozen model match the selected complete Demo broker ledger at a particular cutoff timestamp; rejects stale or future-dated training rows.
- Does **not** provide a cryptographic digest or byte-for-byte equality of reward values; those require a separate immutable model artifact.
- Does **not** establish independent OOS, train/test embargo for dynamically adapted systems, MT5 execution profitability, sample sufficiency or broker fills for the research trial.
- Continues to report `independent_oos=false`, `broker_pnl=null`, and `promotion_enabled=false` by design; the future direction measurement is not a trade win rate.
- No changes to account binding, commission configuration, small-balance flexibility, risk gates or real trading permissions.

## Verification matrix
- Rust unit tests: unrefreshed weights, valid Demo ledger, source counts, REAL refusal, future-dated closed sample refusal.
- React source-contract tests: account-scoped cutoff and refusal to display training results as OOS/PNL.
- Existing P1-K5A Classic M1 and P1-K5B directional tests stay in `npm test`.
- Require exact-head GitHub Research, canonical V25 Portable and Windows legacy checks before release.
- Real Windows UI interaction, MetaEditor native EA and actual MT5 Demo broker acceptance remain **not tested** here.

## Next planned stage
P1-K5D: durable, checksum-verified frozen research session artifact with explicit train/test origin; then predeclared chronological evaluation, independent holdout across sessions, and a separate broker-cost/fill comparison. Do not automatically promote a model based on a directional score.
