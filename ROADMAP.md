# BrainTrade OS — Roadmap

GitHub `gngoongoog/V4` is canonical. Read `BUILD_HANDOFF.md` and `docs/QUALITY_AUDIT.md` before editing.

## Current completed research foundation

- Historical Meta v1/v2, CES v1 and KTE v1 with frozen saved data, offline replay, experiments and detailed ledgers.
- Offline per-trade candlestick review and deterministic report/export replay.
- Observational Trend, Regime, Structure, Volatility, Gann, broker DOM Depth and Macro/News context.
- Persisted chronological Meta v1/v2 evaluation over immutable saved sources.
- Learning snapshots/labels/export, deterministic offline baseline trainer and immutable challenger registry.
- Manual challenger review eligibility, read-only challenger inference and exact matured-label prediction outcome scoring.
- Repository quality guards for broker read-only behavior, no learned execution/promotion route, source/package hygiene, SQLite FK integrity and Windows CI on `development`.

Legacy Meta still controls live Shadow. No broker execution exists.

## Next scientific layer — challenger performance review

Build a **manual research review policy** over persisted scored prediction outcomes. It must remain observational and must not create/activate a champion.

1. Require immutable model/dataset/review identity and sufficiently large scored samples per target/context.
2. Evaluate class support, confusion, balanced accuracy, macro-F1, log loss, Brier and calibration with explicit small-sample rejection/warnings.
3. Add chronological/regime/context slices without selecting thresholds from the same held-out evidence being judged.
4. Persist a signed/versioned review record with exact input outcome IDs / boundaries and reason codes.
5. Expose read-only status/report APIs and a native UI review surface.
6. Add deterministic tests for insufficient samples, missing classes, duplicated/reordered outcomes, context mismatch and reproducibility.
7. Keep `promotion_enabled=false`, `shadow_effect=false`, `broker_effect=false` everywhere.

A favorable review may say “eligible for human consideration” only. Champion activation remains a later, separate design and validation stage.

## Real-data research and walk-forward work

- Collect enough real broker completed-bar observations and exact matured labels.
- Compare baseline/challenger behavior across chronological windows and market regimes.
- Extend walk-forward evaluation without leakage, random shuffling or automatic parameter selection on holdout results.
- Add ablation checks to distinguish real signal contribution from correlated duplicate features.
- Only after reproducible evidence exists, consider a versioned model packaging path such as ONNX for more complex offline-trained challengers.

## Native acceptance

When the Windows laptop/MT5 environment is available:

- Run the full native acceptance checklist against the user's actual demo broker/account.
- Verify historical retrieval, Market Book availability/absence behavior, account switching, restart persistence and all UI pages.
- Test private Telegram delivery only after credentials are configured locally.
- Treat GitHub Windows packaging as build validation only; it is not broker acceptance.

## Data/market extensions

- True feed-supported order flow and volume profile only when the data source actually provides those fields; do not relabel candle tick volume or CFD Market Book as global exchange flow.
- Correlation/portfolio research with aligned timestamps and explicit availability masks.
- Historical news/event replay with first-observed timestamps and no-lookahead joins.

## Reliability / release work

- Full backup restore with manifest/hash/schema validation, staged restore and rollback.
- The 0.4.0 package version is now aligned with Cargo/Cargo.lock/UI. Complete the requested portable delivery after fresh release validation.
- Enable GitHub branch protection / required CI checks for `main` and preferably `development` when repository settings access is available.
- Promote a safe/resumable `development` checkpoint to `main` only after deliberate review; do not fast-forward merely because CI is green.

## Execution — only after separate explicit validation

- Broker order state machine with persistent idempotent intent IDs.
- Reconciliation after timeout/restart; never blind-resubmit an unknown order.
- Hard deterministic risk gate, broker contract checks, hedging/netting behavior and partial-fill fixtures.
- Demo acceptance before any optional live mode.

No deadline or profitability guarantee is implied. A module is complete only when code, verification and state documentation agree.
