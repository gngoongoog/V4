# BrainTrade OS — continue from post-0.6 development

The reviewed 0.6.0 line is already promoted to `main` through PR #13. Its verified runtime checkpoint is `fdc63d4d05df89e27607641bd197b9e6ead936e9`; Research CI `38803539711` and Windows CI/package `38803539405` passed on that checkpoint. Do not rewrite `main` or `development` history.

Portfolio Exposure v1 is no longer pending: PR #16 merged to `development` as `fd8490e6f33f3bd61e655829c12e7824462694a1`. Exact feature head `f36b9c1c7bd265675f5802efa8d2eddfc27c1ee8` passed Linux + Windows run `38807773183` before merge.

Portfolio protection is OFF by default. When enabled it caps aggregate BrainTrade entry exposure across all identities by combined positions/unresolved opens, lots and estimated stop-risk percentage. Unknown aggregate stop risk fails closed for new entries only. Exits, SL/TP, timed management and reconciliation stay active. Configure it while STOPPED from the native Execution page.

The next repository task is the post-0.6 development/release boundary: preserve `docs/validation.json` as the promoted 0.6 evidence, retain `docs/validation-v0.6.0.json` as its archive, and only create a new-version validation record after that new-version code is actually tested. The Windows preview artifact upload has already been made version-agnostic on `release/0.7-preview`.

Operational gates still pending outside CI:

- native Windows + real Demo MT5 acceptance using `docs/ACCEPTANCE.md`;
- private Telegram delivery with local-only credentials;
- repository required checks / branch protection when administration access is available.

Scientific work still open includes genuine out-of-sample accumulation, ablation, aligned correlation/portfolio research and historical-event replay with first-observed timestamps. Imported ML models remain research-only; do not auto-promote them or claim profitability, HFT latency or native broker acceptance from CI evidence.
