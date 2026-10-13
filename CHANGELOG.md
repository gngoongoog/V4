# Changelog

## 0.4.0 — in progress

- Added KTE signal module from the supplied defaults.
- Added stateful KTE historical execution with three-leg partial exits.
- Added TP1/TP2/TP3 handling, next-bar break-even, ATR trailing, 45/90-minute time limits and three-bar cooldown.
- Added conservative stop-first handling when stop and target are both reachable inside an unknown OHLC range, with ambiguity persisted.
- Added deterministic KTE regression coverage for partial sizing, stop management, time limits, ambiguity and cooldown.
- Added KTE to the historical strategy selector.
- Made the saved `Trade` ledger (`fills`, `stop_updates`, `targets`) the common source for saved-report text and detailed KTE reporting.
- Added `fills.csv`, `stop_updates.csv` and `targets.csv` while preserving `trades.csv` as the compatible trade-summary export.
- Added HTML KTE ledger output and fill markers on the equity SVG from the same saved fill records.
- Added real-core integration checks that detailed ledger exports exist and unchanged offline replay reproduces them byte-for-byte.
- Fixed Windows portable ZIP durability step by opening the temporary archive read/write before `fsync`; final Windows CI confirmation is still pending at this checkpoint.
- Fresh Linux CI on the current 0.4.0 path: 25 domain Rust tests passed, 14 runtime Rust tests passed with one live-network test ignored, 16 Python tests passed, core build and real-core smoke passed.
- Durable working source remains `gngoongoog/V4` as normal files/folders; `.import`, encoded staging and chunk transport are not part of the working tree.
- `Cargo.lock` is generated normally by Cargo.

## 0.3.0 — validated source preview baseline

- Historical Meta/CES lab and asynchronous MT5 history requests.
- Persistent reports/raw inputs and deterministic offline replay.
- Same-data experiments and CSV/JSON/HTML exports with equity curve.
- Official Fed/ECB RSS page/cache and Telegram `/news` + `/backtest` read-only handlers.
- Prior recorded validation: 18 domain Rust tests, 13 runtime Rust tests, 16 Python tests, core smoke and Linux desktop render/build checks.

## 0.2.0 — last delivered Windows portable preview

- Last portable Windows package delivered before native Windows/MT5 acceptance was deferred.
