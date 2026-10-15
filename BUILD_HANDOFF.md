# BrainTrade OS — 0.7.0 development preview handoff

Canonical repository: `gngoongoog/V4`.

## Promoted release: 0.6.0

Verified executable/source checkpoint: `fdc63d4d05df89e27607641bd197b9e6ead936e9` (tree `8593783f43a4892cdc724ed5d112607ef7cee006`). Research CI `38803539711` and Windows CI/package `38803539405` passed on that exact checkpoint. PR #13 promoted the reviewed 0.6.0 line to `main` without rewriting history.

Authoritative promoted-release evidence remains `docs/validation.json`; `docs/validation-v0.6.0.json` is its immutable archive. Native MT5 Demo acceptance and live Telegram delivery were not performed by CI.

## Current development: 0.7.0 Preview

Workspace development version is `0.7.0`. Its evidence is deliberately separate in `docs/development-validation.json`. That record starts as `pending-ci`; no 0.7 tested SHA or successful CI run may be claimed until the exact 0.7 source head passes both Research and Windows validation.

Portfolio Exposure v1 is the first post-0.6 execution feature. PR #16 merged to `development` as `fd8490e6f33f3bd61e655829c12e7824462694a1`; exact feature head `f36b9c1c7bd265675f5802efa8d2eddfc27c1ee8` passed Linux + Windows run `38807773183`. The gate remains OFF by default and limits new aggregate BrainTrade entries by exposure slots, lots and estimated stop-risk percentage. Unknown aggregate stop risk fails closed for new entries only; exits, broker SL/TP, timed management and reconciliation remain active.

The Windows preview artifact workflow is version-agnostic. The 0.7 line also adds exact-head development validation rather than reusing 0.6 evidence.

## Operational boundaries

- Startup/restart remains STOPPED.
- Secondary SOLO never arms global START.
- Native MT5 demo acceptance on the target machine remains pending and must be recorded separately from CI.
- Live Telegram delivery remains pending and credentials must stay local.
- Imported ML challengers remain research-only; no automatic Champion promotion or broker activation exists.
- CI/package/simulated-MT5 evidence is not a profitability, HFT-latency or execution-quality guarantee.

## Next engineering work

1. Finish the 0.7.0 version/lock/UI/document boundary and run exact-head Research + Windows CI.
2. Freeze `docs/development-validation.json` to the exact verified 0.7 source SHA only after those checks pass.
3. Run native Windows + Demo MT5 acceptance on the target installation.
4. Test private Telegram delivery with local-only credentials.
5. Continue out-of-sample, ablation, aligned multi-symbol correlation/portfolio and historical-event replay research with no-lookahead boundaries.

Do not force-update or rewrite `main`/`development` history.
