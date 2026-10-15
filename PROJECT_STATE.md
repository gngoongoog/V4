# BrainTrade OS — project state after 0.6.0 promotion

## Stable promoted baseline

The verified 0.6.0 executable/source checkpoint is `fdc63d4d05df89e27607641bd197b9e6ead936e9` (tree `8593783f43a4892cdc724ed5d112607ef7cee006`). Research CI `38803539711` and Windows CI/package `38803539405` passed on that checkpoint, and PR #13 promoted the reviewed line to `main` without history rewriting.

The 0.6 baseline includes 14 execution identities, coordinator/advisor/SOLO/RETURNING roles, Scalping/Swing/Expert/Sniper modes, persistent broker intents/reconciliation, schema-12 backup support, adaptive member weighting, the historical research/learning pipeline and an 18-page native dashboard. Recovery hardening covers unknown-intent account isolation, cumulative split-fill reconciliation without top-up/resend, durable-intent identity mismatch fail-closed behavior and preservation of `broker-journal.sqlite` outside staged restore.

## Development beyond 0.6

Portfolio Exposure v1 was merged through PR #16 to `development` as `fd8490e6f33f3bd61e655829c12e7824462694a1`. The exact feature head `f36b9c1c7bd265675f5802efa8d2eddfc27c1ee8` passed Linux and Windows jobs in run `38807773183` before merge.

The portfolio gate is OFF by default. When enabled it aggregates BrainTrade-owned positions and unresolved open intents, prevents double counting once a broker position is visible, deterministically admits simultaneous queued entries, and can cap total exposure slots, total lots and estimated stop risk as a percentage of equity. Unknown aggregate stop risk blocks only new entries; exits, broker SL/TP, timed management and reconciliation continue. The settings are persisted, included in backup/restore and editable while STOPPED from the native Execution page.

Current release evidence for the promoted 0.6 line remains `docs/validation.json`; the post-0.6 preparation branch archives the same evidence as `docs/validation-v0.6.0.json` before introducing a new development-version validation record.

Release boundary remains unchanged: startup/restart is STOPPED, imported ML challengers remain research-only with no automatic promotion, CI execution acceptance is simulated, native MT5 Demo acceptance has not yet been performed on the target machine, and live Telegram delivery is still pending.

See `BUILD_HANDOFF.md` for the current continuation point, `ROADMAP.md` for remaining work and `docs/EXECUTION_AND_ROLES.md` for execution semantics.
