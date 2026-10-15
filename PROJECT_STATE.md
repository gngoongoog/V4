# BrainTrade OS 0.6.0 Preview

The verified executable/source checkpoint for the 0.6 development candidate is `fdc63d4d05df89e27607641bd197b9e6ead936e9` (tree `8593783f43a4892cdc724ed5d112607ef7cee006`). Research CI `38803539711` and Windows CI/package `38803539405` both passed on that checkpoint.

Implemented: 14 execution identities, coordinator/advisor/SOLO/RETURNING roles, Scalping/Swing/Expert/Sniper modes, persistent intent/reconciliation state, schema-12 backup support, adaptive member weighting, the historical research/learning pipeline, and an 18-page native dashboard.

Recovery hardening now explicitly covers unknown-intent account isolation, cumulative split-fill reconciliation without top-up/resend, durable-intent identity mismatch fail-closed behavior, and preservation of `broker-journal.sqlite` outside staged backup restore.

Release boundary: startup/restart is STOPPED. Imported ML challengers remain research-only and cannot auto-promote. CI execution acceptance is simulated and never connects to a broker. Native MT5 demo-account acceptance and live Telegram delivery have not been performed in the build environment.

The reviewed promotion path is PR #13 from `development` to `main`; branches must not be force-updated or history-rewritten. Release-record/documentation commits may sit on top of the tested source checkpoint without changing runtime behavior.

See `docs/validation.json` for the tested commit, CI runs and Windows artifact hashes, and `docs/EXECUTION_AND_ROLES.md` for execution semantics.
