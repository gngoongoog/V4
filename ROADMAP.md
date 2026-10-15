# BrainTrade OS — Roadmap

GitHub `gngoongoog/V4` is canonical. Read `BUILD_HANDOFF.md` and `docs/validation.json` before editing.

## Completed research foundation

- Historical Meta v1/v2, CES v1 and KTE v1 with saved immutable data, replay, experiments and detailed ledgers.
- Offline candlestick trade review and deterministic exports.
- Observational Trend, Regime, Structure, Volatility, Gann, broker DOM Depth and Macro/News context.
- Persisted chronological evaluation.
- Learning snapshots/labels/export, offline baseline trainer and immutable challenger registry.
- Manual challenger review eligibility, read-only inference and exact matured-label scoring.
- **Challenger Performance Review Policy v1** over immutable scored outcomes, with fixed evidence gates, baselines, chronological/context/regime slices, calibration/stability/drift checks, immutable review records/reason codes, read-only API/UI, and no Champion/Shadow/broker effect.
- Repository guards for read-only MT5 behavior, no learned execution/promotion route, source/package hygiene, SQLite FK integrity and Windows CI.

Validated executable checkpoint: `42c8c6dfda42acfa02de733f3acb93ff639075cc`.

Expert uses legacy Meta; selected Scalping/Swing/Sniper profiles also drive Shadow. Research synthesis and Wyckoff learning remain observational. No broker execution exists.

## Immediate next scientific work — real OOS evidence

1. Accumulate enough real broker completed-bar observations and exact matured labels to satisfy the fixed performance-review sample/context/regime gates.
2. Compare behavior across chronological windows and regimes without selecting/tuning thresholds from the same judged holdout evidence.
3. Add ablation checks to separate useful signal from correlated duplicate features.
4. Keep performance-review decisions manual/read-only. A favorable result means human consideration only.
5. If the policy itself must change, create a new version with rationale and re-evaluate on fresh evidence; do not silently retune v1 after seeing results.

A small test-hardening item remains useful: add an explicit dedicated missing-class fixture in addition to the implemented class-support fail-closed rule.

## Native acceptance

When the target Windows/MT5 environment is available:

- run the full native acceptance checklist against the actual demo broker/account;
- verify historical retrieval, Market Book availability/absence behavior, account switching, restart persistence and all UI pages;
- test private Telegram delivery only with locally configured credentials;
- treat GitHub Windows packaging as build validation, not broker acceptance.

## Data / market extensions

- True feed-supported order flow and volume profile only when the source actually provides those fields.
- Correlation/portfolio research with aligned timestamps and availability masks.
- Historical news/event replay with first-observed timestamps and no-lookahead joins.

## Reliability / release work

- Completed: backup restore with manifest/hash/schema validation, staged restart and rollback, native controls and packaged Windows verification.
- Enable GitHub branch protection / required CI checks for `main` and preferably `development` when repository administration access is available.
- Promote a safe `development` checkpoint to `main` only after deliberate review.

## Execution — only after separate explicit design and validation

- Persistent broker order state machine with idempotent intent IDs.
- Reconciliation after timeout/restart; never blind-resubmit an unknown order.
- Deterministic risk gate, broker contract checks, hedging/netting behavior and partial-fill fixtures.
- Demo acceptance before any optional live mode.
- Champion activation/promotion, if ever added, must be a separate manual architecture and validation stage.

No deadline or profitability guarantee is implied. A module is complete only when code, verification and state documentation agree.
