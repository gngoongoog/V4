# Changelog

## 0.4.0 — in progress

- Added KTE signal module from the user's supplied defaults.
- Added stateful KTE historical execution path.
- Added TP1/TP2/TP3 partial-position handling, break-even/trailing behavior and cooldown logic.
- Added KTE to the historical strategy selector.
- Moved the durable working source to GitHub repository `gngoongoog/V4` as normal files and folders; legacy `.import` staging is not part of the working tree.
- Added `PROJECT_STATE.md`, `BUILD_HANDOFF.md` and `ROADMAP.md` as resume/checkpoint documents.
- `Cargo.lock` is generated normally by Cargo from the committed manifests.
- Python gateway/packaging tests: 16/16 passed during the earlier migration checkpoint; fresh 0.4.0 CI validation is tracked separately.

## 0.3.0 — validated source preview baseline

- Historical Meta/CES lab and asynchronous MT5 history requests.
- Persistent reports/raw inputs and deterministic offline replay.
- Same-data experiments and CSV/JSON/HTML exports with equity curve.
- Official Fed/ECB RSS page/cache and Telegram `/news` + `/backtest` read-only handlers.
- Prior recorded validation: 18 domain Rust tests, 13 runtime Rust tests, 16 Python tests, core smoke and Linux desktop render/build checks.

## 0.2.0 — last delivered Windows portable preview

- Last portable Windows package delivered before packaging was deferred.
