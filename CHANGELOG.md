# Changelog

## 0.4.0 — in progress

- Added KTE signal module from the user's supplied defaults.
- Added stateful KTE historical execution path.
- Added TP1/TP2/TP3 partial-position handling, break-even/trailing behavior and cooldown logic.
- Added KTE to the historical strategy selector.
- Migrated the source checkpoint to GitHub repository `gngoongoog/BrainTrade-OS` for durable continuation.
- Added `PROJECT_STATE.md` and `ROADMAP.md` as resume/checkpoint documents.
- Python gateway/packaging tests: 16/16 passed during migration.

## 0.3.0 — validated source preview baseline

- Historical Meta/CES lab and asynchronous MT5 history requests.
- Persistent reports/raw inputs and deterministic offline replay.
- Same-data experiments and CSV/JSON/HTML exports with equity curve.
- Official Fed/ECB RSS page/cache and Telegram `/news` + `/backtest` read-only handlers.
- Prior recorded validation: 18 domain Rust tests, 13 runtime Rust tests, 16 Python tests, core smoke and Linux desktop render/build checks.

## 0.2.0 — last delivered Windows portable preview

- Last portable Windows package delivered before packaging was deferred.
