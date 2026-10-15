# BrainTrade OS 0.6.0 Preview — verified development candidate

Canonical repository: `gngoongoog/V4`.

Verified executable/source checkpoint: `fdc63d4d05df89e27607641bd197b9e6ead936e9` (tree `8593783f43a4892cdc724ed5d112607ef7cee006`).

Validation on that exact checkpoint:

- Research CI: PASS — run `38803539711`.
- Windows CI/package: PASS — run `38803539405`.
- Windows artifact: `BrainTrade-Windows-x64-v0.6.0-Preview`, 33,948,849 bytes, SHA-256 `3f7a945a4f9c1ab1ce3e008880722c3dc078a54018a8f622133c604420665779`.
- Windows native-validation artifact: 1,785,325 bytes, SHA-256 `64725943777396d8c974c825a0319554b7f6536213709fd4d76edd49c6a6c66d`.
- Simulated MT5 execution integration: PASS; the harness does not connect to a broker.
- Native broker/account acceptance: NOT PERFORMED.
- Telegram live delivery: NOT PERFORMED.

Implemented in the 0.6 line:

- 14 execution identities with coordinator/advisor/SOLO/RETURNING membership.
- Four trading modes: Scalping, Swing, Expert/adaptive and live-quote Sniper.
- Persistent execution intents, reconciliation, partial-fill handling, exact account/ticket ownership checks and schema-12 backup support.
- Adaptive member weighting from prospective 12-bar outcomes; imported ML challengers remain a separate research-only pipeline with no automatic promotion.
- Native dashboard with 18 pages.
- Startup and restart are STOPPED; a secondary SOLO toggle does not arm global START.

Latest verified recovery/execution hardening:

- reconciled fills stay blocked until the owned position is actually observed;
- exact submitted broker volume is persisted;
- Sniper applies the configured broker-cost threshold;
- unknown intents stay scoped to the bound account across an account switch;
- split fills reconcile cumulatively without a top-up or duplicate resend;
- a reused durable intent ID with changed execution identity fails closed;
- schema-12 restore keeps the durable gateway send journal outside rollback, and regression coverage verifies the journal survives restore unchanged.

Release/promotion state:

- `development` contains the reviewed 0.6 candidate.
- `main` and `development` have a divergent historical merge topology; never force-update or rewrite either branch to promote 0.6.
- PR #13 is the deliberate promotion path from `development` to `main`.
- Release-record/documentation commits may sit on top of the tested source checkpoint. They must not silently change runtime behavior; `docs/validation.json` records the executable checkpoint and artifact evidence.

Next engineering work after promotion:

1. Run the native Windows + real Demo MT5 acceptance checklist on the target installation and record it separately from CI.
2. Test private Telegram delivery using only local credentials that never enter the repository.
3. Enable branch protection / required checks for `main` and preferably `development` when repository administration policy is available.
4. Design and validate portfolio-wide exposure management as a separate execution change; do not imply it is already present.
5. Continue genuine out-of-sample evidence, ablation, correlation/portfolio research and historical-event replay without weakening the current no-lookahead boundaries.

No CI result, package build or simulated broker fixture establishes profitability, guaranteed latency or native broker acceptance.
