# BrainTrade OS V4 — 0.7.2 Unified Windows Demo Release Candidate

**Package:** `BrainTrade-Windows-x64-v0.7.2-Preview.zip` (a Windows x64 portable ZIP contained within the GitHub Actions downloadable artifact `BrainTrade-Windows-x64-Preview`). This is the combined version for **controlled MT5 Demo acceptance**, not an independently certified profitable trading product.

## Included in this unified build

- Complete Windows `BrainTrade.exe`, Rust Core and embedded Python MT5 gateway, together with the native Sniper EA source and read-only Demo acceptance collector.
- Sixteen legacy durable execution identities preserved; `sniper_learner` owns **410617** and **410616** remains reserved, unowned and explicitly rejected by MT5.
- Expert multi-brain coordinator, Scalping, Swing, Sniper classic, and the dedicated tick-driven Sniper Learner with direct operator choice, strict selected-mode-only execution and separate shadow training, Demo learning, and explainable signals.
- Configurable fixed net-pip target or four user-bounded, context-specific **Demo-only** pip alternatives. Only exact closed broker outcomes with matching submitted TP geometry update the chosen target's learner; broker-widened TP is excluded from arm scoring.
- Adaptive contextual loss-memory, limited exploration, journaled broker sends, partial and unknown-reply reconciliation, cost and margin checks, account-binding persistence before activation, exposure accounting for break-even/trailing stops.
- Sniper/Scalping M1/M5 readiness independent from unrelated M15 research readiness; closed/finite/fresh bar constraints still apply.
- Windows launcher `Run-Demo-Acceptance.cmd`, Arabic Sniper guide `START_HERE_SNIPER_DEMO_AR.md` and a complete `Source` archive for independent review.

## Release qualification — must use exact-source test evidence

The Windows and Research workflow checks validate Rust/Python test suites, simulated broker replies, source integrity and packaging. Download only a successful Windows artifact from `main` or an explicitly reviewed audit PR with an identical source tree to the promoted commit. Never use an incomplete build or a ZIP with unknown provenance. The package carries `package-validation.json` and `SHA256SUMS.json`; check their source commit and archive contents against GitHub Actions.

A successful automated test suite does **not** validate:
- actual fills, partial-close behavior or latency on your broker's Windows MT5 Demo installation;
- execution under price gaps, unstable ticks, network failures not represented in tests;
- future profitability, the best possible pip target, or zero bugs.

The target-machine MT5 Demo acceptance step is mandatory before considering any Live use. Start in **STOPPED** mode, confirm the target account is Demo, and verify symbol, volume minimum, margin, roundtrip commission and spread. Never change to a real account merely to get more trades.

See `START_HERE_SNIPER_DEMO_AR.md`, `README_AR.md`, and `Run-Demo-Acceptance.cmd`.

## Version history

Version 0.7.0 with the smoke-gray control center is promoted to `main` as of October 20, 2026. Version 0.6.0 is the historical predecessor. Broker-specific MT5 Demo acceptance and out-of-sample profitability validation remain outstanding. A package of 0.7.0 may be feature-complete for Demo trial without being certified as production-safe or consistently profitable. Research-only ML challengers, speculative entry-timing comparisons and news sentiment are not broker execution authorities.

## REAL switch and Core isolation in 0.7.2

The 0.7.2 version bump prevents the native interface from connecting to a stale 0.7.1 Core. REAL binding requires explicit exact MT5 login confirmation and fresh telemetry. Each REAL START must separately confirm the bound login, account trade permissions and enabled portfolio exposure gate. No broker-live acceptance or profitability has been established.

## Brain control and Sniper follow-up

The 22nd native page adds account/symbol/mode-scoped advisor and paper-entry controls with sampled activity, strategy-attributed order acknowledgements, and separate broker-owner history. Four learner styles preserve cost accounting and isolate learned evidence. Telegram system responses and command buttons are Arabic. See `BRAIN_CONTROL_SNIPER_AR.md`; target-machine MT5 acceptance remains outstanding.
