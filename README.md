# BRAINTRADE OS — 0.8.0 Source Preview

Native Windows desktop with an embedded Core and Python MT5 bridge. Launch `BrainTrade.exe`; set the broker terminal path, connect the saved Demo account, bind it on Execution and press START. Initial startup and restart are STOPPED.

The 18-page dashboard includes four modes (Scalping, Swing, adaptive Expert, live-quote Sniper), proportional commission and live spread calculation, account/position status, analysis and opportunity history, market map, Shadow research, risk settings, historical backtests, chronological evaluation, imported-model research, official news, Telegram queries, backups, 21-timeframe strategy generation, Wyckoff forward learning and broker execution.

0.7 introduced Portfolio Exposure v1: an optional, default-OFF aggregate gate across all BrainTrade identities for combined exposure slots, lots and estimated stop-risk percentage. Unknown aggregate stop risk blocks new entries fail-closed when the gate is enabled; existing exits, broker SL/TP, timed management and reconciliation remain active.

The coordinator uses the actual advisor set. SOLO removes a secondary brain from its votes and enables independent execution; Return rejoins after any owned position/unsettled intent is resolved. Each brain has its own configuration, magic identity and broker-observed performance. Global START/STOP is independent from membership. Optional per-brain risk budgets and the aggregate portfolio gate default off; ownership, account matching and duplicate prevention remain mandatory.

The promoted 0.6.0 evidence remains in [validation.json](docs/validation.json). The verified 0.7 Windows preview is archived in [validation-v0.7.0.json](docs/validation-v0.7.0.json). Current [0.8 source checks](docs/development-validation.json) do **not** certify a Windows binary: this update was made without building or triggering CI. See [the 0.8 change notes](docs/FAST_EXECUTION_UI.md).

See [execution and roles](docs/EXECUTION_AND_ROLES.md), [modes and research](docs/TRADING_MODES_AND_STUDIO.md) and [Arabic quick start](docs/QUICKSTART_AR.md). Native MT5/Telegram acceptance must be completed on the user's broker/device. Simulated execution tests and compilation do not establish profitability or guaranteed latency.

Build: `scripts/build-windows.ps1` on Windows x64 with Rust stable, Visual Studio C++ tools and Python 3.13. End users need none of these build tools. Pinned embedded dependencies, full source and notices are included in the ZIP.
