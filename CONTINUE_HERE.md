# BrainTrade OS — continue from 0.7.0 Development Preview

The reviewed 0.6.0 line is promoted to `main` through PR #13. Its verified checkpoint is `fdc63d4d05df89e27607641bd197b9e6ead936e9`; promoted evidence stays in `docs/validation.json` and `docs/validation-v0.6.0.json`.

Portfolio Exposure v1 is merged to `development` through PR #16 (`fd8490e6f33f3bd61e655829c12e7824462694a1`). Exact feature head `f36b9c1c7bd265675f5802efa8d2eddfc27c1ee8` passed Linux + Windows run `38807773183`. Portfolio protection remains OFF by default and affects new aggregate entries only.

Current workspace development version is `0.7.0`. `docs/development-validation.json` is the only validation record allowed to describe 0.7. It starts as `pending-ci`; do not copy 0.6 SHA/run/artifact evidence into it. Freeze it to `verified` only after the exact 0.7 source head passes both Research and Windows checks.

Immediate continuation:

- finish Cargo.lock alignment for the three BrainTrade workspace packages;
- align current-development UI/docs/tooling with 0.7.0 while keeping release-history files at 0.6.0;
- enable Research CI on pull requests and avoid stale concurrent PR runs;
- run exact-head Research + Windows validation and record only those results.

External acceptance still pending: native Windows + real Demo MT5 using `docs/ACCEPTANCE.md`, private Telegram delivery with local credentials, and repository required checks/branch protection when administration access is available.

Do not rewrite `main` or `development` history. Do not claim profitability, HFT latency or native broker acceptance from CI evidence.
