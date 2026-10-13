# BRAINTRADE OS — Research 0.3.0 — Source Preview

Native Rust/Slint desktop with independent Rust core and read-only MT5 gateway.
Mohammed confirmed v0.1.1 Windows launch, MT5 connection and account balance.

0.3.0 adds a historical tester (Meta/CES), asynchronous MT5 history requests,
persisted reports and raw inputs, deterministic offline replay, strategy
experiments on the same data, and CSV/JSON/HTML export with an equity curve.
Official Fed/ECB RSS headlines have their own page, cached-source health and
refresh controls. Telegram gains /news and /backtest read-only reports.
Existing closed-bar analysis, shadow simulation and risk settings remain.

**Windows packaging is deferred at the user's request. This is source, not a
new runnable Windows ZIP. The last delivered portable is 0.2.0.**

No broker orders, modifications, autonomous protection or trained models.
START allows virtual entries; STOP leaves virtual exits active while Core runs.
Old configuration reads with risk defaults. Full Exit from old Core before upgrade.

- Arabic setup/upgrade: docs/QUICKSTART_AR.md
- Live Meta/shadow rules: docs/RESEARCH.md
- Historical Meta/CES: docs/BACKTEST.md
- News sources and freshness: docs/NEWS.md
- Full roadmap (not completed features): docs/ARCHITECTURE.md
- Validation boundaries: docs/RELEASE_STATUS.md and docs/validation.json

## Developer build

Rust stable (tested with 1.99), Python 3.13 x64 on Windows, Visual Studio C++ build
tools. Run `scripts/build-windows.ps1` from PowerShell. This is a **build-machine**
step, not the intended end-user launch process. It creates a portable ZIP in
`release/`; final users launch `BrainTrade.exe` without installing Rust/Python.
The workflow can produce the same ZIP on a Windows runner.

Linux core tests:

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

For a source gateway on a Windows development machine, set
`BRAINTRADE_GATEWAY_SCRIPT` to its absolute path and `BRAINTRADE_PYTHON` to the
interpreter containing MetaTrader5. The production package finds its bundled
gateway automatically. On Linux, no MT5 feed is available; the dashboard shows
that absence explicitly. Linux desktop compilation needs pkg-config and the fontconfig development library.

Data defaults to the current user's OS application data folder. Never put live
tokens or account passwords in this repository. `BRAINTRADE_DATA_DIR` exists for
isolated tests and should point to a private user-owned directory.

## Portable delivery

BrainTrade.exe + isolated embedded CPython 3.13.16, MT5/NumPy wheels, complete
Source/ and third-party notices. Final users need Windows 10/11 x64 and installed
MT5; no system Rust/Python. Preserve the whole extracted directory.
Package with `python scripts/package-portable.py --exe PATH/BrainTrade.exe`;
the build machine needs pefile==2024.8.26. Assets are pinned and hash-checked.

New 0.3.0 native features still need testing on the user's Windows broker setup.
No Telegram network test without credentials. No real MT5 orders.
KTE/Gann, true order-flow/DOM, trained ML/ONNX, news-informed decisions,
walk-forward optimization, portfolio management and full restore remain roadmap.
See docs/RELEASE_STATUS.md and docs/validation.json for the exact tested boundary.
