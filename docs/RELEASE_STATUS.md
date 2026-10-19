# BrainTrade OS V4 — 0.7.0 Unified Windows Demo Release Candidate

**Package:** `BrainTrade-Windows-x64-v0.7.0-Preview.zip` (a Windows x64 portable ZIP contained within the GitHub Actions downloadable artifact `BrainTrade-Windows-x64-Preview`). This is the combined version for **controlled MT5 Demo acceptance**, not an independently certified profitable trading product.

## Included in this unified build

- Complete Windows `BrainTrade.exe`, Rust Core and embedded Python MT5 gateway, together with the native Sniper EA source and read-only Demo acceptance collector.
- Sixteen legacy durable execution identities preserved; `sniper_learner` owns **410617** and **410616** remains reserved, unowned and explicitly rejected by MT5.
- Expert multi-brain coordinator, Scalping, Swing, Sniper classic, and the dedicated tick-driven Sniper Learner with direct operator choice, independent/returning SOLO behavior, Demo learning, and explainable signals.
- Configurable fixed net-pip target or four user-bounded, context-specific **Demo-only** pip alternatives. Only exact closed broker outcomes with matching submitted TP geometry update the chosen target's learner; broker-widened TP is excluded from arm scoring.
- Adaptive contextual loss-memory, limited exploration, journaled broker sends, partial and unknown-reply reconciliation, cost and margin checks, account-binding persistence before activation, exposure accounting for break-even/trailing stops.
- Sniper/Scalping M1/M5 readiness independent from unrelated M15 research readiness; closed/finite/fresh bar constraints still apply.
- Windows launcher `Run-Demo-Acceptance.cmd`, Arabic Sniper guide `START_HERE_SNIPER_DEMO_AR.md` and a complete `Source` archive for independent review.

## Release qualification — must use exact-source test evidence

The Windows and Research workflow checks validate Rust/Python test suites, simulated broker replies, source integrity and packaging. Download **only the artifact from a successful Windows run on the unified PR #69 branch or its merged development commit**. The package carries `package-validation.json` and `SHA256SUMS.json`; check their source commit and archive contents against GitHub Actions.

A successful automated test suite does **not** validate:
- actual fills, partial-close behavior or latency on your broker's Windows MT5 Demo installation;
- execution under price gaps, unstable ticks, network failures not represented in tests;
- future profitability, the best possible pip target, or zero bugs.

The target-machine MT5 Demo acceptance step is mandatory before considering any Live use. Start in **STOPPED** mode, confirm the target account is Demo, and verify symbol, volume minimum, margin, roundtrip commission and spread. Never change to a real account merely to get more trades.

See `START_HERE_SNIPER_DEMO_AR.md`, `README_AR.md`, and `Run-Demo-Acceptance.cmd`.

## Version history

Version 0.6.0 remains the old promoted baseline on `main`. Version 0.7.0 is the unified development release candidate. A package of 0.7.0 may be feature-complete for Demo trial without being certified as production-safe or consistently profitable. Research-only ML challengers, speculative entry-timing comparisons and news sentiment are not broker execution authorities.
