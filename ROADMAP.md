# BrainTrade OS — Roadmap

GitHub `gngoongoog/V4` is canonical. Read `BUILD_HANDOFF.md`, `PROJECT_STATE.md` and both release/development validation records before editing.

## Promoted 0.6.0 checkpoint

Verified executable/source checkpoint: `fdc63d4d05df89e27607641bd197b9e6ead936e9` (tree `8593783f43a4892cdc724ed5d112607ef7cee006`). Research CI `38803539711`: PASS. Windows CI/package `38803539405`: PASS. PR #13 promoted the reviewed 0.6.0 line to `main` without history rewriting.

The promoted line contains bound-account execution, 14 execution identities, coordinator/advisor/SOLO/RETURNING membership, Scalping/Swing/Expert/Sniper, adaptive member weighting, schema-12 backup/restore and the 18-page native dashboard. Startup/restart is STOPPED. CI uses simulated MT5; native MT5 demo acceptance remains a separate target-machine gate.

## Current 0.8.0 source-only preview

Fast pulse/IPC, isolated history reader, one-click Demo, expansion brain and dashboard regrouping are in source. The user requested no build; `docs/development-validation.json` records only source checks. Next engineering verification needs an explicitly requested build, then native MT5 demo acceptance. No new Windows package was produced.

## Archived 0.7.0 Development Preview

`0.7.0` is CI-verified at test checkout `ba399ad206b1fe3231a10dbbd270de75145fd2e4` (same tree as PR head `e0547cab6ccce8d41944750f88d0089dc689ff29`). Research `38817498736` and Windows/Linux `38817498472` passed on that checkpoint; `docs/validation-v0.7.0.json` records its own evidence. Historical 0.6 release records remain unchanged.

Completed 0.7 engineering work: workspace/lockfile/UI version alignment with external dependency pins preserved; version-agnostic portable checks/artifacts; Research PR validation; stale-run cancellation; and exact-source evidence. Portfolio Exposure v1 is included, OFF by default, with persisted aggregate entry limits and a stopped-only native editor.

## Completed research foundation

- Historical Meta v1/v2, CES v1 and KTE v1 with immutable inputs, replay, experiments and ledgers.
- Offline candlestick review and deterministic exports.
- Trend, Regime, Structure, Volatility, Gann, broker DOM Depth and Macro/News context.
- Chronological evaluation and no-lookahead research guards.
- Learning snapshots/labels/export, offline baseline trainer, challenger registry and manual review/performance evidence.
- Durable execution intent/reconciliation, partial-fill handling and account isolation.
- Portfolio Exposure v1 persistence, backup/restore and native editor.

## Immediate engineering work

1. Run native Windows + Demo MT5 acceptance using `docs/ACCEPTANCE.md` and record it separately from CI.
2. Test private Telegram delivery only with local credentials.
3. Enable branch protection / required checks for `main` and preferably `development` when repository administration access is available.
4. Begin aligned multi-symbol correlation/exposure research with timestamp availability masks before considering any correlation-driven execution rule.

## Scientific work still open

- Accumulate genuine completed-bar observations and matured labels for out-of-sample evaluation.
- Add ablation checks to separate useful signal from correlated duplicate features.
- Add aligned correlation/portfolio research with explicit missing-data masks and no forward filling across unavailable intervals.
- Add historical news/event replay with first-observed timestamps and no-lookahead joins.
- True feed-supported order flow and volume profile only when the source actually provides those fields.

## Explicitly not complete

- Native broker acceptance on the user's actual MT5 installation.
- Live Telegram acceptance.
- Imported ML Champion activation or automatic promotion.
- Correlation-driven broker execution policy.
- Any profitability, HFT-latency or execution-quality guarantee.

A module is complete only when code, verification and state documentation agree.
