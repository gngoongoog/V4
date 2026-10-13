# Research source checkpoint — offline trade review

Development contains the existing KTE partial execution engine and observational
Trend/Regime, Structure/Volatility and Meta v2, plus the new offline per-trade
candlestick review. The report uses saved history and the same execution ledger
as JSON/CSV. Tests and exact source commit are in BUILD_HANDOFF.md and validation.json.

Research validation covers 33 domain and 14 runtime unit tests (one explicit
network test ignored), 16 Python tests, the actual Rust core with synthetic MT5
inputs, KTE three-target exports, deterministic offline replay and browser
interactions at 1200/880 widths. No remote chart requests are made.

A failed local toolchain install did not execute Rust tests; the reported Rust
and integration results come from GitHub's Linux research job. New Windows
packaging is deferred. The historical Meta v2 CI run on adf94f8 was successful,
but its Windows artifact is not a new declared user release or laptop acceptance.
The last delivered Windows portable remains 0.2.0 Preview.

Native broker history on the user's laptop and live Telegram delivery with local
credentials remain untested for these additions. Broker order submission remains
disabled. The next code task is a selectable historical Meta v2 comparison on
identical saved data and costs; live Shadow still uses legacy Meta.

Software fixture results establish implementation behavior, not profitability,
TradingView equivalence or real broker execution quality.
