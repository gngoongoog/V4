# BrainTrade OS 0.6.0 Preview — verified development checkpoint

Canonical repository: `gngoongoog/V4`.

Current verified development source: `a60d8201a52630a577e8120d5f4c852f9743a9d3`.

Validation on that exact source:

- Research CI: PASS — run `38770422025`.
- Windows CI/package: PASS — run `38770422066`.
- Windows artifact: `BrainTrade-Windows-x64-v0.6.0-Preview`, SHA-256 `8bd9a7f120826d5ab6d02af2936998d914e5e372bcd332ee8ee2753bad2279bc`.
- Simulated MT5 execution integration: PASS; the harness does not connect to a broker.
- Native broker/account acceptance: NOT PERFORMED.
- Telegram live delivery: NOT PERFORMED.

Implemented in the 0.6 development line:

- 14 execution identities with coordinator/advisor/SOLO/RETURNING membership.
- Four trading modes: Scalping, Swing, Expert/adaptive and live-quote Sniper.
- Persistent execution intents, reconciliation, partial-fill handling, exact account/ticket ownership checks and schema-12 backup support.
- Adaptive member weighting from prospective 12-bar outcomes; imported ML challengers remain a separate research-only pipeline with no automatic promotion.
- Native dashboard with 18 pages.
- Startup and restart are STOPPED; a secondary SOLO toggle does not arm global START.

Latest fixes before this checkpoint:

- reconciled fills stay blocked until the owned position is actually observed;
- exact submitted broker volume is persisted;
- Sniper applies the configured broker-cost threshold;
- schema-12 restore keeps the durable gateway send journal outside rollback so an old backup cannot recreate a sent-order journal entry.

Repository state:

- `main` is still the older promoted 0.4 research checkpoint.
- `development` contains the 0.6 work and must not be fast-forwarded blindly because the branches have diverged by the prior merge topology.
- The next promotion should use a reviewed merge/PR after the 0.6 documentation and CI state are reconciled.

Next safe engineering work:

1. Keep release/version/validation files mutually consistent and guarded by CI.
2. Reconcile `main` and `development` through a reviewed merge rather than rewriting history.
3. Continue deterministic simulation/regression coverage for restart, account switching, reconciliation, backups and UI state.
4. Run the native demo-account acceptance checklist only on the target Windows/MT5 installation; record its result separately from CI.
5. Keep imported ML challenger activation and any automatic promotion disabled unless a separately reviewed design and fresh evidence justify it.
