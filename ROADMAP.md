# BrainTrade OS — Roadmap

GitHub `gngoongoog/V4` is canonical. Read `BUILD_HANDOFF.md` and `docs/validation.json` before editing.

## Current 0.6 development checkpoint

Verified executable/source checkpoint: `a60d8201a52630a577e8120d5f4c852f9743a9d3`.

Research CI `38770422025`: PASS. Windows CI/package `38770422066`: PASS.

The active `development` source contains bound-account execution code, 14 execution identities, coordinator/advisor/SOLO/RETURNING membership, four trading modes, adaptive member weighting, schema-12 backup/restore support and an 18-page native dashboard. Startup/restart is STOPPED. The CI execution harness uses a simulated MT5 object and never connects to a broker; native MT5 demo acceptance is still separate and pending.

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
- Latest regression fixes preserve the position-observation barrier after reconciliation and persist the exact submitted volume.

## Immediate quality/release work

1. Keep Cargo version, release status, handoff and validation evidence consistent through CI guards.
2. Reconcile `main` and `development` through a reviewed merge/PR; do not force-update the diverged branches.
3. Expand deterministic simulation fixtures for restart, account switching, reconciliation, backup/restore and dashboard state.
4. Run the native Windows + demo MT5 acceptance checklist on the target machine and record it separately from CI.
5. Test private Telegram delivery only with local credentials that never enter the repository.
6. Enable branch protection / required checks for `main` and preferably `development` when repository administration policy is finalized.

## Scientific work still open

- Accumulate real completed-bar observations and matured labels for genuine out-of-sample evaluation.
- Add ablation checks to separate useful signal from correlated duplicate features.
- Add correlation/portfolio research with aligned timestamps and availability masks.
- Add historical news/event replay with first-observed timestamps and no-lookahead joins.
- True feed-supported order flow and volume profile only when the source provides those fields.

## Explicitly not complete

- Native broker acceptance on the user's actual MT5 installation.
- Live Telegram acceptance.
- Portfolio-wide exposure management across all identities.
- Imported ML Champion activation or automatic promotion.
- Any profitability, HFT-latency or execution-quality guarantee.

A module is complete only when code, verification and state documentation agree.
