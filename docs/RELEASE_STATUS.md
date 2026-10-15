# BrainTrade OS 0.6.0 Preview

The current `development` branch contains the 0.6 execution/research stack and has passed both Research CI and Windows packaging on source commit `a60d8201a52630a577e8120d5f4c852f9743a9d3`.

Implemented: 14 execution identities, coordinator/advisor/SOLO/RETURNING roles, Scalping/Swing/Expert/Sniper modes, persistent intent/reconciliation state, schema-12 backup support, adaptive member weighting, the existing historical research/learning pipeline, and an 18-page native dashboard.

Release boundary: startup/restart is STOPPED. Imported ML challengers remain research-only and cannot auto-promote. CI execution acceptance is simulated and never connects to a broker. Native MT5 demo-account acceptance and live Telegram delivery have not been performed in the build environment.

`main` remains the older promoted 0.4 checkpoint. `development` is the active 0.6 line and is currently ahead with a divergent merge topology, so promotion must be reviewed rather than force-updated.

See `docs/validation.json` for the exact tested commit, CI runs and Windows artifact hashes, and `docs/EXECUTION_AND_ROLES.md` for execution semantics.
