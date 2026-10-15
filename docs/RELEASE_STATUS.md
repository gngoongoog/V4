# BrainTrade OS 0.6.0 Preview

The verified executable/source checkpoint for the 0.6 development candidate is `fdc63d4d05df89e27607641bd197b9e6ead936e9` (tree `8593783f43a4892cdc724ed5d112607ef7cee006`). Research CI `38803539711` and Windows CI/package `38803539405` both passed on that checkpoint.

The packaged Windows artifact is `BrainTrade-Windows-x64-v0.6.0-Preview` (33,948,849 bytes, SHA-256 `3f7a945a4f9c1ab1ce3e008880722c3dc078a54018a8f622133c604420665779`). The Windows native-validation artifact is 1,785,325 bytes with SHA-256 `64725943777396d8c974c825a0319554b7f6536213709fd4d76edd49c6a6c66d`.

Implemented: 14 execution identities, coordinator/advisor/SOLO/RETURNING roles, Scalping/Swing/Expert/Sniper modes, persistent intent/reconciliation state, schema-12 backup support, adaptive member weighting, the historical research/learning pipeline, and an 18-page native dashboard.

Recovery hardening covers unknown-intent account isolation, split-fill reconciliation without top-up/resend, durable-intent identity mismatch fail-closed behavior, and preservation of the gateway send journal outside staged backup restore.

Release boundary: startup/restart is STOPPED. Imported ML challengers remain research-only and cannot auto-promote. CI execution acceptance is simulated and never connects to a broker. Native MT5 demo-account acceptance and live Telegram delivery have not been performed in the build environment.

Promotion to `main` must use the reviewed PR path rather than force-updating the diverged branches. Release-record/documentation commits may sit on top of the tested checkpoint without changing runtime behavior.

See `docs/validation.json` for machine-readable evidence and `docs/EXECUTION_AND_ROLES.md` for execution semantics.
