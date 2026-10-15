# BrainTrade OS — Roadmap

GitHub `gngoongoog/V4` is canonical. Read `BUILD_HANDOFF.md` and `docs/validation.json` before editing.

## Promoted 0.6 checkpoint

Verified executable/source checkpoint: `fdc63d4d05df89e27607641bd197b9e6ead936e9` (tree `8593783f43a4892cdc724ed5d112607ef7cee006`).

Research CI `38803539711`: PASS. Windows CI/package `38803539405`: PASS. The reviewed 0.6 line has been promoted to `main` through PR #13 without history rewriting.

The promoted 0.6 source contains bound-account execution code, 14 execution identities, coordinator/advisor/SOLO/RETURNING membership, four trading modes, adaptive member weighting, schema-12 backup/restore support and an 18-page native dashboard. Startup/restart is STOPPED. The CI execution harness uses a simulated MT5 object and never connects to a broker; native MT5 demo acceptance remains separate and pending.

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

## Current development — portfolio exposure v1

PR #16 introduces a separate, default-OFF portfolio entry gate across all 14 execution identities. Current implemented scope on the feature branch:

- Aggregate visible BrainTrade positions and unresolved open intents before admitting new entries.
- De-duplicate a filled/partial intent after the corresponding owned position becomes visible.
- Deterministic admission order for simultaneously queued entries from different brains.
- Optional maximum combined positions/intents, maximum combined lots and maximum estimated stop-risk percentage of current equity.
- Fail closed for new entries when aggregate stop risk cannot be established; never block exits or reconciliation.
- Persist portfolio settings in SQLite and verify staged backup/restore returns them exactly.
- Expose aggregate exposure through `/v1/execution`; the native Execution page displays current exposure state.
- Stopped-only `/v1/execution/portfolio` configuration API. A dedicated native editor for these fields remains the UI follow-up before declaring the desktop workflow complete.

The 0.6 release evidence in `docs/validation.json` remains immutable evidence for the promoted 0.6 source and must not be silently relabeled as validation of this new development feature.

## Immediate engineering work

1. Finish PR #16 CI/review and add the native portfolio-limit editor.
2. Start the post-0.6 development version line (0.7 Preview) only after release/version/workflow/document guards are reconciled together.
3. Run the native Windows + Demo MT5 acceptance checklist on the target machine and record it separately from CI.
4. Test private Telegram delivery only with local credentials that never enter the repository.
5. Enable branch protection / required checks for `main` and preferably `development` when repository administration policy is available.

## Scientific work still open

- Accumulate real completed-bar observations and matured labels for genuine out-of-sample evaluation.
- Add ablation checks to separate useful signal from correlated duplicate features.
- Add correlation/portfolio research with aligned timestamps and availability masks.
- Add historical news/event replay with first-observed timestamps and no-lookahead joins.
- True feed-supported order flow and volume profile only when the source provides those fields.

## Explicitly not complete

- Native broker acceptance on the user's actual MT5 installation.
- Live Telegram acceptance.
- Native editor for portfolio exposure limits.
- Imported ML Champion activation or automatic promotion.
- Any profitability, HFT-latency or execution-quality guarantee.

A module is complete only when code, verification and state documentation agree.
