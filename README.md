# BRAINTRADE OS — 0.7.2 Demo / REAL Confirmation Review

Current follow-up: [Brain control room and Sniper styles](docs/BRAIN_CONTROL_SNIPER_AR.md). Adds a 22nd page, scoped controls and Arabic Telegram reports. Historical 0.7.0 audit files below describe earlier revisions.

Native Windows desktop with an embedded Core and Python MT5 bridge. Launch `BrainTrade.exe`; set the broker terminal path, connect the saved Demo account, bind it on Execution and press START. Initial startup and restart are STOPPED.

The 22-page dashboard includes four modes (Scalping, Swing, adaptive Expert, live-quote Sniper), proportional commission and live spread calculation, account/position status, analysis and opportunity history, market map, Shadow research, risk settings, historical backtests, chronological evaluation, imported-model research, official news, Telegram queries, backups, 21-timeframe strategy generation, Wyckoff forward learning and broker execution.

0.7 development adds Portfolio Exposure v1: an optional, default-OFF aggregate gate across all BrainTrade identities for combined exposure slots, lots and estimated stop-risk percentage. Unknown aggregate stop risk blocks new entries fail-closed when the gate is enabled; existing exits, broker SL/TP, timed management and reconciliation remain active.

Only the system coordinator of the **selected mode** may submit new broker entries. Secondary brains remain analytic / Shadow-Paper observers; historical SOLO switches are disabled. Expert alone may use advisor votes for its entries; other modes cannot be overridden by opposing slow advisor votes. Each brain has an identity and evidence counters. Paper outcomes are sampled, simulated observations and do **not** directly train broker-execution weights. The Sniper learner uses matched, fully closed MT5 Demo evidence. Global START/STOP is explicit and initial startup is STOPPED; ownership, account matching and duplicate prevention remain mandatory.

The promoted 0.6.0 evidence in [validation.json](docs/validation.json) is historical. Version 0.7.2 build acceptance is recorded per exact CI commit, and code/integration findings are in [the October 21 audit](docs/AUDIT_2026_10_21_EXECUTION_LEARNING_INTEGRITY.md). Never treat prior CI as proof that a new commit passed or as native broker/Telegram acceptance.

See [execution and roles](docs/EXECUTION_AND_ROLES.md), [modes and research](docs/TRADING_MODES_AND_STUDIO.md) and [Arabic quick start](docs/QUICKSTART_AR.md). Native MT5/Telegram acceptance must be completed on the user's broker/device. Simulated execution tests and compilation do not establish profitability or guaranteed latency.

Build: `scripts/build-windows.ps1` on Windows x64 with Rust stable, Visual Studio C++ tools and Python 3.13. End users need none of these build tools. Pinned embedded dependencies, full source and notices are included in the ZIP.

### Demo student learning

The Trading → Learning progress page records entry context and exact booked Demo outcomes, and keeps quote-only exit experiments separate. Flexible Expert uses context-aware epsilon-greedy selection among actionable advisors; exploration decreases from 30% toward 5% as broker evidence grows. The chosen action probability is logged; it is not a probability of making money. Only participating Expert advisors enter coordinator voting; secondary brains have no independent broker-entry authority.

Three exit profiles compare the original geometry, a quicker exit, and a more patient target. Twenty complete paired observations nominate a challenger; twenty **newly opened** trades validate it. Adoption affects future Demo entries only, and continued deterioration returns to the original profile. Disable automatic application in Trading → Mode and costs while stopped. Learning does not impose a minimum sample count before entering trades.

See [docs/STUDENT_LEARNING.md](docs/STUDENT_LEARNING.md) for exact evidence rules, limitations and acceptance checks.
