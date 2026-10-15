# BrainTrade OS 0.6.0 Preview

Current 0.6 source contains actual bound-account broker execution, 14 identities, adaptive coordinator membership, SOLO/RETURNING transitions, four modes and an 18-page native dashboard. Optional risk limits are off by default; start/restart is STOPPED.

Verified executable/source checkpoint: `fdc63d4d05df89e27607641bd197b9e6ead936e9` (tree `8593783f43a4892cdc724ed5d112607ef7cee006`). Research CI `38803539711` and Windows CI/package `38803539405` passed. The authoritative artifact hashes and release boundaries are in `docs/validation.json`.

Recovery coverage now includes account-switch isolation for unknown intents, cumulative split-fill reconciliation without resend, intent-identity mismatch fail-closed behavior and preservation of the durable gateway send journal across staged backup restore.

CI broker acceptance is simulated and never connects to a broker. Native MT5 Demo acceptance and live Telegram delivery remain unperformed and must be recorded separately.

If PR #13 is still open, use it as the reviewed promotion path from `development` to `main`; never force-update the diverged histories. If promotion is already complete, the next operational gate is native Windows + Demo MT5 acceptance using `docs/ACCEPTANCE.md`, followed by private Telegram delivery testing.

Do not enable imported-ML automatic promotion or claim portfolio-wide exposure management unless those are implemented and separately validated.
