# BrainTrade OS — Roadmap

GitHub `gngoongoog/V4` is canonical. Read `BUILD_HANDOFF.md`, `PROJECT_STATE.md` and both release/development validation records before editing.

## Promoted 0.6.0 checkpoint

Verified executable/source checkpoint: `fdc63d4d05df89e27607641bd197b9e6ead936e9` (tree `8593783f43a4892cdc724ed5d112607ef7cee006`). Research CI `38803539711`: PASS. Windows CI/package `38803539405`: PASS. PR #13 promoted the reviewed 0.6.0 line to `main` without history rewriting.

The promoted line contains bound-account execution, 14 execution identities, coordinator/advisor/SOLO/RETURNING membership, Scalping/Swing/Expert/Sniper, adaptive member weighting, schema-12 backup/restore and the 18-page native dashboard. Startup/restart is STOPPED. CI uses simulated MT5; native MT5 demo acceptance remains a separate target-machine gate.

## Current 0.7.0 Development Preview

Current `development` runtime/source merge checkpoint is `7b127bf03fb45044ee53dc2366a6d0c53e7c59d8` (tree `10d1d6bee091b02fa1d044336649e645bce1b177`). PR #20 head `58e41284334d6778a3bfd2aa7eb5dd9cca5628fd` has the same tree and passed exact-head Research and Windows/Linux validation before merge. Historical 0.6 release records remain unchanged.

Completed 0.7 engineering work now includes workspace/lockfile/UI version alignment with external dependency pins preserved; version-agnostic portable checks/artifacts; exact-head PR validation and stale-run cancellation; Portfolio Exposure v1; the Sniper Fast Lane; and the native Sniper MT5 OnTick/CopyTicks quote bridge with authenticated local transport, source-order recovery, same-millisecond event separation and 5 ms Python fallback.

The Sniper local engineering target is <=20 ms from BrainTrade queue to `order_send` handoff on an appropriate target machine. Component timing is recorded for characterization. This is not a guaranteed MT5/broker acknowledgement, fill latency, network latency, profitability or HFT claim.

## Completed research foundation

- Historical Meta v1/v2, CES v1 and KTE v1 with immutable inputs, replay, experiments and ledgers.
- Offline candlestick review and deterministic exports.
- Trend, Regime, Structure, Volatility, Gann, broker DOM Depth and Macro/News context.
- Chronological evaluation and no-lookahead research guards.
- Learning snapshots/labels/export, offline baseline trainer, challenger registry and manual review/performance evidence.
- Durable execution intent/reconciliation, partial-fill handling and account isolation.
- Portfolio Exposure v1 persistence, backup/restore and native editor.
- Sniper local fast-path timing instrumentation and native terminal-cached quote recovery.

## Immediate engineering work

1. Complete native Windows + Demo MT5 acceptance using `docs/ACCEPTANCE.md` and record it separately from CI, including native/fallback Sniper P50/P95/P99 latency characterization.
2. Test private Telegram delivery only with local credentials.
3. Enable branch protection / required checks for `main` and preferably `development` when repository administration access is available.
4. Build aligned multi-symbol correlation/exposure **research only** with exact timestamp alignment and explicit availability masks before considering any correlation-driven execution rule.

## Next research phase — aligned multi-symbol correlation/exposure

The first implementation must remain observational and must not change broker execution, Portfolio Exposure v1 or coordinator decisions.

- Select multiple immutable saved source runs with distinct symbols and preserved account/server/contract identity.
- Align exact M1 timestamps; never forward-fill a missing symbol through unavailable intervals.
- Persist an explicit availability mask per symbol/timestamp.
- Compute return/covariance/correlation/volatility only over pairwise valid intersections, with an explicit minimum-overlap threshold. Insufficient overlap is `unavailable`, never synthetic zero correlation.
- Persist deterministic research identity from ordered source identities and configuration.
- Add deterministic replay/export and restart tests.
- Test positive, negative and near-zero fixtures, gaps, asynchronous availability, same timestamps, insufficient overlap and no-lookahead behavior.
- Keep any correlation-driven broker policy explicitly unimplemented until separate prospective evidence and review justify it.

## Scientific work still open

- Accumulate genuine completed-bar observations and matured labels for out-of-sample evaluation.
- Add ablation checks to separate useful signal from correlated duplicate features.
- Complete aligned correlation/portfolio research with explicit missing-data masks and no forward filling across unavailable intervals.
- Add historical news/event replay with first-observed timestamps and no-lookahead joins.
- True feed-supported order flow and volume profile only when the source actually provides those fields.

## Explicitly not complete

- Native broker acceptance on the user's actual MT5 installation.
- Live Telegram acceptance.
- Imported ML Champion activation or automatic promotion.
- Correlation-driven broker execution policy.
- Guaranteed capture of every upstream broker event.
- Any profitability, HFT-latency or execution-quality guarantee.

A module is complete only when code, verification and state documentation agree.
