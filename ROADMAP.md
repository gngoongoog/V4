# BrainTrade OS — Roadmap

GitHub `gngoongoog/V4` is canonical. Read `BUILD_HANDOFF.md`, `PROJECT_STATE.md` and the validation records before editing.

## Promoted 0.6 checkpoint

Verified executable/source checkpoint: `fdc63d4d05df89e27607641bd197b9e6ead936e9` (tree `8593783f43a4892cdc724ed5d112607ef7cee006`).

Research CI `38803539711`: PASS. Windows CI/package `38803539405`: PASS. The reviewed 0.6.0 line was promoted to `main` through PR #13 without history rewriting.

The promoted 0.6 source contains bound-account execution code, 14 execution identities, coordinator/advisor/SOLO/RETURNING membership, four trading modes, adaptive member weighting, schema-12 backup/restore support and an 18-page native dashboard. Startup/restart is STOPPED. CI uses a simulated MT5 object and does not establish native broker acceptance.

Imported ML challengers remain research-only. There is no automatic Champion promotion or learned-model broker activation.

## Completed research foundation

- Historical Meta v1/v2, CES v1 and KTE v1 with immutable saved inputs, replay, experiments and detailed ledgers.
- Offline candlestick trade review and deterministic exports.
- Observational Trend, Regime, Structure, Volatility, Gann, broker DOM Depth and Macro/News context.
- Persisted chronological evaluation.
- Learning snapshots/labels/export, offline baseline trainer and immutable challenger registry.
- Manual challenger review eligibility, read-only inference, exact matured-label scoring and performance-review policy.
- Repository guards for source/package hygiene, SQLite FK integrity, causal learning order and Windows CI.

## Completed 0.6 engineering work

- Durable execution intent state, gateway send journal, timeout/restart reconciliation and partial-fill handling.
- Exact account/ticket ownership checks and hedging/netting ownership rules.
- Four modes: Scalping, Swing, Expert/adaptive and live-quote Sniper.
- Fourteen configured identities with SOLO exclusion, RETURNING ownership and automatic rejoin after settlement.
- Prospective 12-bar outcomes and persisted adaptive member weights.
- Backup schema 12 with application-state rollback while the durable gateway send journal remains outside restore rollback.
- Reconciled fills remain behind the broker-position observation barrier and preserve exact submitted volume.
- Unknown intents remain bound to the original account across account switches.
- Split fills reconcile cumulatively without automatic top-up or duplicate submission.
- Reusing an intent ID with changed execution identity fails closed.
- Backup/restore regression coverage verifies `broker-journal.sqlite` remains outside rollback and unchanged.

## Completed post-0.6 engineering — portfolio exposure v1

PR #16 merged to `development` as `fd8490e6f33f3bd61e655829c12e7824462694a1`. Exact feature head `f36b9c1c7bd265675f5802efa8d2eddfc27c1ee8` passed Linux and Windows run `38807773183`.

Implemented scope:

- Default-OFF aggregate entry gate across all BrainTrade execution identities.
- Aggregate visible owned positions and unresolved open intents before admitting new entries.
- De-duplicate a filled/partial intent after the corresponding owned position becomes visible.
- Deterministic admission order for simultaneously queued entries.
- Optional maximum combined positions/intents, maximum combined lots and maximum estimated stop-risk percentage of current equity.
- Fail closed for new entries when aggregate stop risk cannot be established; never block exits, SL/TP, timed management or reconciliation.
- Persist portfolio settings in SQLite and restore them through staged backup/restore.
- Expose aggregate state through `/v1/execution` and a STOPPED-only `/v1/execution/portfolio` configuration endpoint.
- Native Execution-page editor for enable/disable, exposure slots, lots and stop-risk percentage.

The feature-build artifacts are evidence for the exact PR #16 head, not a new numbered release. Promoted 0.6 evidence remains separate and is archived as `docs/validation-v0.6.0.json` on the post-0.6 preparation line.

## Immediate engineering work

1. Reconcile the post-0.6 development version line and validation model without relabeling old 0.6 CI as new-version evidence.
2. Run the native Windows + Demo MT5 acceptance checklist on the target machine and record it separately from CI.
3. Test private Telegram delivery only with local credentials that never enter the repository.
4. Enable branch protection / required checks for `main` and preferably `development` when repository administration policy is available.
5. Add the next research-quality portfolio layer: aligned multi-symbol correlation/exposure research with timestamp availability masks before any correlation-based execution rule is considered.

## Scientific work still open

- Accumulate real completed-bar observations and matured labels for genuine out-of-sample evaluation.
- Add ablation checks to separate useful signal from correlated duplicate features.
- Add aligned correlation/portfolio research with explicit missing-data availability masks and no forward filling across unavailable source intervals.
- Add historical news/event replay with first-observed timestamps and no-lookahead joins.
- True feed-supported order flow and volume profile only when the source provides those fields.

## Explicitly not complete

- Native broker acceptance on the user's actual MT5 installation.
- Live Telegram acceptance.
- Imported ML Champion activation or automatic promotion.
- Correlation-driven broker execution policy.
- Any profitability, HFT-latency or execution-quality guarantee.

A module is complete only when code, verification and state documentation agree.
