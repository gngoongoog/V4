# BrainTrade OS — project state: 0.7.0 development preview

## Promoted baseline: 0.6.0

The verified 0.6.0 executable/source checkpoint is `fdc63d4d05df89e27607641bd197b9e6ead936e9` (tree `8593783f43a4892cdc724ed5d112607ef7cee006`). Research CI `38803539711` and Windows CI/package `38803539405` passed on that checkpoint, and PR #13 promoted the reviewed line to `main` without history rewriting.

Authoritative release evidence remains `docs/validation.json`, archived unchanged as `docs/validation-v0.6.0.json`.

## Current development: 0.7.0

The workspace has moved to `0.7.0` Development Preview. Its validation state is separate in `docs/development-validation.json`; while that record says `pending-ci`, no tested 0.7 commit or successful 0.7 CI is being claimed.

Portfolio Exposure v1 was merged through PR #16 to `development` as `fd8490e6f33f3bd61e655829c12e7824462694a1`. Exact feature head `f36b9c1c7bd265675f5802efa8d2eddfc27c1ee8` passed Linux and Windows run `38807773183`. The aggregate gate is OFF by default, prevents double counting once a position is visible, and can cap total exposure slots, lots and estimated stop-risk percentage. Unknown aggregate stop risk blocks only new entries; exits and reconciliation continue.

The 0.7 line is also separating development-version evidence from promoted-release evidence and removing hard-coded package version assumptions from CI.

Operational boundary remains unchanged: startup/restart is STOPPED; imported ML challengers remain research-only; CI execution acceptance is simulated; native MT5 demo acceptance and live Telegram delivery remain pending outside CI.

See `BUILD_HANDOFF.md`, `CONTINUE_HERE.md`, `ROADMAP.md`, `docs/EXECUTION_AND_ROLES.md`, `docs/validation.json` and `docs/development-validation.json` before continuing.
