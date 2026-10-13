# BRAINTRADE OS — 0.4.0 Development Checkpoint

Native Rust/Slint desktop with an independent Rust core and read-only MT5 gateway.
The 0.4.0 research milestone is in progress on `development`. GitHub is the
canonical source; BUILD_HANDOFF.md records the current tested checkpoint.
The last delivered Windows portable remains 0.2.0 Preview. Windows packaging
is deferred until requested; CI validation artifacts are not native acceptance.

## Current capabilities

- Native dashboard, independent Core and read-only MT5 bootstrap.
- Start / Stop / reconnect / full-exit controls; persistent Shadow and risk.
- Historical Meta/CES/KTE lab with saved data, offline replay and experiments.
- KTE three partial targets, delayed break-even/trailing and detailed ledgers.
- Local HTML candlestick review for each trade, equity curve and CSV/JSON.
- Observational Trend/Regime, Structure/Volatility and Meta v2 candidate.
- Fed/ECB RSS cache and Telegram read-only queries.

Legacy Meta remains the Shadow decision source. Broker orders are not submitted.
Native MT5 acceptance and live Telegram delivery remain to be tested later.

## Resume here

- `PROJECT_STATE.md` — exact current state and validation boundary.
- `ROADMAP.md` — ordered next work.
- `CHANGELOG.md` — checkpoint history.
- `docs/ARCHITECTURE.md` — broader design/roadmap, not a completed-feature list.
- `docs/BACKTEST.md` — historical Meta/CES/KTE assumptions and offline trade review.
- `docs/NEWS.md` — news sources and freshness behavior.

## Developer build

Rust stable (prior source tested with Rust 1.99), Python 3.13 x64 on Windows and
Visual Studio C++ build tools are expected for the Windows build machine.

Linux/core verification commands:

```sh
cargo test -p braintrade-domain -p braintrade-runtime --locked
cargo build -p braintrade-runtime --bin braintrade-core --locked
python3 -m unittest discover -s tests -v
python3 scripts/smoke_core.py
```

Development desktop:

```sh
cargo run -p braintrade-desktop
```

For a source gateway on Windows, set `BRAINTRADE_GATEWAY_SCRIPT` to its absolute
path and `BRAINTRADE_PYTHON` to the interpreter containing MetaTrader5. Production
packaging finds its bundled gateway automatically.

## Secrets and local data

Do not commit live Telegram tokens, API keys, broker passwords, MT5 credentials,
private databases or runtime logs. Runtime data should stay in the user's OS app-data
folder (or a private `BRAINTRADE_DATA_DIR`).

## Portable delivery

Packaging scripts are preserved for later use, but **no new 0.4.0 Windows portable is
claimed here**. The previous delivery remains 0.2.0 Preview until a new Windows build
is explicitly requested and tested.
