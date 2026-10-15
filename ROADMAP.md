# BrainTrade OS — Roadmap

GitHub `gngoongoog/V4` is canonical. Read `BUILD_HANDOFF.md`, `PROJECT_STATE.md` and both release/development validation records before editing.

## Promoted 0.6.0 checkpoint

Verified executable/source checkpoint: `fdc63d4d05df89e27607641bd197b9e6ead936e9` (tree `8593783f43a4892cdc724ed5d112607ef7cee006`). Research CI `38803539711`: PASS. Windows CI/package `38803539405`: PASS. PR #13 promoted the reviewed 0.6.0 line to `main` without history rewriting.

The promoted line contains bound-account execution, 14 execution identities, coordinator/advisor/SOLO/RETURNING membership, Scalping/Swing/Expert/Sniper, adaptive member weighting, schema-12 backup/restore and the 18-page native dashboard. Startup/restart is STOPPED. CI uses simulated MT5; native MT5 demo acceptance remains a separate target-machine gate.

## Current 0.7.0 Development Preview

`0.7.0` is the current development line. `docs/development-validation.json` begins `pending-ci`; it must not inherit successful 0.6 SHA/run claims. `docs/validation.json` and `docs/validation-v0.6.0.json` remain the immutable promoted-release evidence.

Portfolio Exposure v1 is merged through PR #16. Exact feature head `f36b9c1c7bd265675f5802efa8d2eddfc27c1ee8` passed Linux + Windows run `38807773183`. The gate is default-OFF and caps aggregate new-entry exposure by slots, lots and estimated stop-risk percentage while preserving exits/reconciliation.

Current 0.7 engineering boundary work:

- align Cargo workspace + lockfile + current-development UI/docs;
- keep historical 0.6 release records unchanged;
- make preview artifact upload version-agnostic;
- run Research validation on pull requests so release evidence belongs to the exact PR SHA;
- cancel stale concurrent runs for superseded PR heads;
- freeze `docs/development-validation.json` only after exact-head Research + Windows success.

## Completed research foundation

- Historical Meta v1/v2, CES v1 and KTE v1 with immutable inputs, replay, experiments and ledgers.
- Offline candlestick review and deterministic exports.
- Trend, Regime, Structure, Volatility, Gann, broker DOM Depth and Macro/News context.
- Chronological evaluation and no-lookahead research guards.
- Learning snapshots/labels/export, offline baseline trainer, challenger registry and manual review/performance evidence.
- Durable execution intent/reconciliation, partial-fill handling and account isolation.
- Portfolio Exposure v1 persistence, backup/restore and native editor.

## Immediate engineering work

1. Complete and verify the `0.7.0` version boundary on an exact PR head with Research + Windows CI.
2. Record that exact 0.7 source SHA/runs/artifacts in `docs/development-validation.json`; do not relabel 0.6 evidence.
3. Run native Windows + Demo MT5 acceptance using `docs/ACCEPTANCE.md` and record it separately from CI.
4. Test private Telegram delivery only with local credentials.
5. Enable branch protection / required checks for `main` and preferably `development` when repository administration access is available.
6. Begin aligned multi-symbol correlation/exposure research with timestamp availability masks before considering any correlation-driven execution rule.

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
