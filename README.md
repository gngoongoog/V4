# BRAINTRADE OS — 0.7.0 Development Preview

Native Windows desktop with an embedded Core and Python MT5 bridge. Launch `BrainTrade.exe`; set the broker terminal path, connect the saved Demo account, bind it on Execution and press START. Initial startup and restart are STOPPED.

The 20-page dashboard includes four modes (Scalping, Swing, adaptive Expert, live-quote Sniper), proportional commission and live spread calculation, account/position status, analysis and opportunity history, market map, Shadow research, risk settings, historical backtests, chronological evaluation, imported-model research, official news, Telegram queries, backups, 21-timeframe strategy generation, Wyckoff forward learning and broker execution.

0.7 development adds Portfolio Exposure v1: an optional, default-OFF aggregate gate across all BrainTrade identities for combined exposure slots, lots and estimated stop-risk percentage. Unknown aggregate stop risk blocks new entries fail-closed when the gate is enabled; existing exits, broker SL/TP, timed management and reconciliation remain active.

The coordinator uses the actual advisor set. SOLO removes a secondary brain from its votes and enables independent execution; Return rejoins after any owned position/unsettled intent is resolved. Each brain has its own configuration, magic identity and broker-observed performance. Global START/STOP is independent from membership. Optional per-brain risk budgets and the aggregate portfolio gate default off; ownership, account matching and duplicate prevention remain mandatory.

The promoted 0.6.0 release evidence remains in [validation.json](docs/validation.json). Current 0.7 development evidence is separate in [development-validation.json](docs/development-validation.json) and must not claim success until the exact 0.7 source head passes the required Research and Windows checks.

See [execution and roles](docs/EXECUTION_AND_ROLES.md), [modes and research](docs/TRADING_MODES_AND_STUDIO.md) and [Arabic quick start](docs/QUICKSTART_AR.md). Native MT5/Telegram acceptance must be completed on the user's broker/device. Simulated execution tests and compilation do not establish profitability or guaranteed latency.

Build: `scripts/build-windows.ps1` on Windows x64 with Rust stable, Visual Studio C++ tools and Python 3.13. End users need none of these build tools. Pinned embedded dependencies, full source and notices are included in the ZIP.

### Demo student learning

The Trading → Learning progress page records entry context and exact booked Demo outcomes, and keeps quote-only exit experiments separate. Flexible Expert uses context-aware epsilon-greedy selection among actionable advisors; exploration decreases from 30% toward 5% as broker evidence grows. The chosen action probability is logged; it is not a probability of making money. SOLO advisors remain excluded from the coordinator.

Three exit profiles compare the original geometry, a quicker exit, and a more patient target. Twenty complete paired observations nominate a challenger; twenty **newly opened** trades validate it. Adoption affects future Demo entries only, and continued deterioration returns to the original profile. Disable automatic application in Trading → Mode and costs while stopped. Learning does not impose a minimum sample count before entering trades.

See [docs/STUDENT_LEARNING.md](docs/STUDENT_LEARNING.md) for exact evidence rules, limitations and acceptance checks.
