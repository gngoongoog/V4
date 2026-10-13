# BRAINTRADE OS — 0.4.0 Development Checkpoint

Native Rust/Slint desktop with an independent Rust core and read-only MT5 gateway.
The current repository is an **in-progress 0.4.0 source checkpoint**. The last fully
validated source baseline is 0.3.0, and the last delivered Windows portable remains
0.2.0 Preview. Windows packaging is deferred until explicitly requested.

## Current capabilities

- Native desktop dashboard and independent Rust Core.
- Read-only MT5 bootstrap/account visibility and completed-bar market data flow.
- Start / Stop / reconnect / full-exit lifecycle controls.
- Live Meta analysis and persistent shadow/risk simulation.
- Historical Meta/CES research lab with asynchronous MT5 history requests.
- Saved raw history + reports, deterministic offline replay and same-data experiments.
- CSV/JSON/HTML exports with equity curve.
- Official Fed/ECB RSS cache and read-only Telegram `/news` + `/backtest` handlers.
- 0.4.0 work-in-progress KTE port with stateful historical partial exits.

KTE is **not yet declared fully validated**. Final regression, chart/export review and
release-artifact reconciliation remain in progress. No broker orders are sent by this
checkpoint.

## Resume here

- `PROJECT_STATE.md` — exact current state and validation boundary.
- `ROADMAP.md` — ordered next work.
- `CHANGELOG.md` — checkpoint history.
- `docs/ARCHITECTURE.md` — broader design/roadmap, not a completed-feature list.
- `docs/BACKTEST.md` — historical Meta/CES assumptions.
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
