> Roadmap, not implemented-feature claims. Current 0.2.0 scope: see RESEARCH.md.

# BRAINTRADE OS — Architecture baseline 0.1

Date: 2026-10-11. Target: Windows x64 desktop, with a later headless VPS core.
This document preserves the full requested scope while distinguishing contracts
already implemented from planned services.

## 1. Process ownership

`BrainTrade.exe` is the only user entry point. It opens the Slint dashboard and
ensures one `BrainTrade.exe --core` process. Packaging uses one binary with two
roles, rather than requiring the user to select Core vs Desktop executables.
The core survives normal UI closure. Core owns gateway, persistence and Telegram.
The UI performs network requests on a worker thread; no HTTP calls in UI callbacks.

v0.1 uses a Python read-only MT5 bootstrap process. This gives official terminal
initialization without EA installation. It is explicitly outside the future
FLASH path. The eventual MQL5/native gateway must expose every execution and
transaction state before execution is enabled. No Python inference, database
write, HTTP, charting, news parsing or Telegram runs on that future hot path.

## 2. Current repository

| Path | Responsibility |
|---|---|
| crates/domain | Configuration, telemetry, lifecycle and report contracts |
| crates/runtime | Independent core, IPC, SQLite audit, gateway supervisor, secrets, Telegram |
| crates/runtime/src/bin | Headless development/test entry point |
| crates/desktop | Single executable launcher + native Slint desktop |
| gateway | Official read-only MT5 bootstrap bridge |
| tests | Bridge unit tests using explicit fake MT5 fixtures |
| scripts | Integration harness and Windows packaging |
| docs | Architecture, setup, implementation boundaries, acceptance |

Planned crates are introduced only when implemented: `market`, `features`,
`brains`, `meta`, `risk`, `execution`, `portfolio`, `recorder`, `replay`, `models`,
`knowledge`, `news`, `backup`, `telemetry`. Planned Python `ai_lab` handles training,
validation and ONNX export independently of the trading core.

## 3. Current lifecycle and future semantics

- Launch: core becomes ready, gateway attempts connection using saved path/account.
- Start: v0.1 requests monitoring. Fresh account snapshot yields Monitoring;
  absent initial snapshot yields Starting; expired/disconnected data yields Degraded.
- Stop: withdraw monitoring intent; connection diagnostics and reports continue.
- UI Close: keeps core alive.
- Full Exit: stop gateway and core, leave MT5 and all broker positions untouched.
- Restart: starts stopped, re-reads configuration, gets a fresh broker snapshot.
- No auto-trading setting exists until execution is implemented and validated.

Future Start requires reconciliation, feed health, permission, selected mode,
model eligibility and hard-risk checks. Future Stop prevents entries while
continuing exit/protection logic for existing positions. Flatten-and-stop is a
separate explicit command. v0.1 does not perform that protection or flattening.

## 4. IPC v1

Current control plane: JSON on fixed loopback `127.0.0.1:18741`. Every route requires
a bearer key. Reject Origin-bearing browser requests. No remote binding, no CORS,
no telemetry upload to external services. Key is DPAPI protected on Windows;
Linux development file is private (0600, directory 0700).

| Method and route | Purpose |
|---|---|
| GET /v1/status | Current health, lifecycle, latest reconciled account snapshot |
| GET /v1/config | Nonsecret configuration and token-present flag |
| POST /v1/config | Validate, persist settings, restart gateway |
| POST /v1/start | Request monitoring |
| POST /v1/stop | Stop monitoring intent |
| POST /v1/reconnect | Reset gateway and invalidate old snapshot |
| POST /v1/gateway/snapshot | Complete bridge snapshot |
| GET /v1/events | Last 100 audited control/connection events |
| POST /v1/backup | Snapshot of settings and audit database only |
| POST /v1/shutdown | Stop background core |

Future remote control needs authenticated encrypted transport and role checks;
the current loopback service must never be exposed with port forwarding.
Future market transport is separate from this low-frequency control plane.

## 5. Persistence

Current SQLite `events` columns: id INTEGER PRIMARY KEY, timestamp_ms INTEGER,
kind TEXT, detail JSON text. WAL + synchronous FULL. Audit includes startup,
commands, connection transitions, settings changes and backup creation. Secrets
are redacted. This is a control audit, **not yet** a complete trading event-sourcing
implementation or raw tick recorder.

Nonsecret config schema 1: mt5_path, symbol, telegram_user_id, telegram_chat_id,
telegram_enabled. Write via same-directory temporary file, fsync and atomic
replacement. Invalid config is preserved and startup fails visibly instead of
silently discarding user settings.

Telegram token uses user-scoped Windows DPAPI, excluded from backups. MT5 login
credentials remain in the existing MT5 terminal in this milestone. Account
password UI/vault and multi-terminal workers are future work.

## 6. Market/account boundaries

Current snapshot: login/server/currency/account type, balance/equity, full open
positions, pending count, today's booked trade-deal net, quote and symbol lot
step/tick size. Missing orders/positions invalidate the snapshot; missing deal
history yields unknown daily PnL. Recheck account identity after collection to
reject account-switch races. Account feed freshness expires after eight seconds;
quote age is a separate field and does not falsely mark a weekend account stale.

Daily report uses Baghdad UTC+03:00 boundaries converted to UTC for MT5. Sum trade
deal profit+commission+swap+fee booked during that day, excluding cash transfers.
It is account-wide, not bot-only. Deal count is not a count of closed trades;
no win rate or drawdown is calculated from insufficient data.

Future terminal identity: account_id + server + terminal_instance_id. Future symbol
identity: internal market ID + broker-native symbol + instrument specification
version. XAUUSD, GOLD, XAUUSDm etc. require explicit verified mapping; do not
assume economically equivalent contracts from names alone.

## 7. Knowledge and model contract (planned)

Each `Knowledge/<market-id>/` capsule has models, replay, news, trades, features,
statistics, regimes, execution and checkpoints. Global knowledge stores broker
execution, general regimes and reusable patterns with provenance. Transfer
between markets requires validation; independent normalizers and versions.

Model manifest: UUID, market/mode/brain, feature schema/hash, software compatibility,
training time span and dataset IDs, label horizon, costs, validation metrics,
parent lineage, weights hash, calibration, champion/challenger role. Confidence
is not called win probability without out-of-sample calibration.

Learning: observations -> replay -> train candidate -> cost-aware backtest ->
out-of-sample -> walk-forward -> deterministic replay -> shadow -> demo -> eligible
champion. Freeze Learning stops training/promotions, not data collection unless
explicitly selected. Reset/rollback are audited and scoped by symbol/brain.

Save Learning: coordinated quiescent checkpoint across weights, optimizer state,
normalizers, replay cursor, news memory, model registry and audit watermark.
Full Backup: versioned manifest, hashes, compatibility, optional encryption;
verify and stage before restore, never resume live execution automatically.
Current settings/audit backup does not implement these learning semantics.

## 8. Brain SDK and decisions (planned)

Input: immutable MarketState + event/receive timestamps + data availability/quality
mask + feature schema + market/mode ID. Output: vote/abstain, score, uncertainty,
horizon, proposed entry/exit context, evidence IDs, model version and health.
Unavailable DOM/news/volume generates abstention, not synthetic observations.

Brain roster: Trend, Regime, Structure, Order Flow, DOM, Volume, Volume Profile,
Liquidity, Momentum, Volatility, Microstructure, Burst, Reversal, Breakout,
Mean Reversion, Session, Correlation, Gann, CES, KTE, News, Macro, Execution,
Exit, Risk Intelligence and Meta. Existing CES/KTE ideas require exact versioned
specifications and tests before porting; names alone are not implementations.

Meta combines nonredundant evidence; it does not independently bypass risk or
submit orders. Correlated votes must not be counted as independent confirmations.
Hard Risk is a deterministic execution gate with explicit reason codes.

Modes: Swing, Intraday, Scalp, Sniper, Flash, Auto, Hybrid; each has distinct
features, horizons, models, entry/exit/execution policy and resource budget.
Separate execution environment: Backtest, Replay, Paper, Shadow, Demo, Live.

## 9. Execution contract (planned)

Intent -> risk approved -> submission pending -> accepted/rejected/unknown ->
partially filled/filled -> managing -> closed. Persist client intent ID before
external side effects. A send timeout means unknown, never automatic blind
resubmission. Reconcile orders/deals/positions with broker history after restart.
MT5 OrderSendAsync receipt is not evidence of fill; OnTradeTransaction is handled
with idempotency and correlation. Hedging/netting, volume step, tick size, filling
mode, stop/freeze levels and market sessions are broker-discovered.

Risk: daily loss, drawdown, open exposure, lot/position limits, spread/slippage,
latency, consecutive loss, sessions, news, hard stop. AI cannot alter these limits.
Measure decision/send/receipt/ack/fill separately with monotonic durations and
timestamp uncertainty; do not promise microsecond end-to-end broker fills.

## 10. News, macro, correlation (planned)

Sources carry attribution, publication time, first observed time and ingest time.
Deduplication -> entity relevance -> importance -> novelty/surprise -> symbol
context. As-of joins prevent lookahead and revised macro data leakage. Measure
market response from first observability; free delayed feeds do not justify
claims of tradable 100 ms news reactions. News state is an immutable summary for
fast inference; parsing/LLM calls stay outside the trading path.

CFD DOM is broker-specific; it does not represent the worldwide gold order book.
Tick volume is labeled separately from exchange traded volume. Correlations use
aligned observations and availability masks; yields/index proxies are labeled.

## 11. Desktop scope

Current: overview, Start/Stop, reconnect, account/positions, Settings with file
picker, Telegram setup, audit and basic backup.

Planned pages: Trading, Charts, Markets, Accounts, Positions, Orders, History,
Brains, Meta, Mode Engine, Learning, Training Lab, Models/Registry, Experience
Replay, News, Economic Events, Regime, Order Flow, DOM, Volume Profile, Liquidity,
Correlations, Backtest, Replay, Walk Forward, Performance, Risk, Execution,
Latency Lab, Brokers, Telegram, Backup/Restore, Health, Logs, Settings.
Pages are added with functional data/actions, not empty cards presenting invented
performance. Chart marks reference immutable trade IDs for historical review.

## 12. Delivery sequence and exit gates

| Milestone | Deliverable | Exit gate |
|---|---|---|
| 1 — current | Core, native dashboard, bootstrap monitoring, settings, Telegram code | Core lifecycle tests; desktop build; native Windows/MT5 verification still required |
| 2 | Native/MQL5 transport, recording, account manager | Disconnect/restart, timestamps, complete broker snapshots |
| 3 | Execution state machine + hard risk | Idempotent intents, no blind retry, hedging/netting and partial-fill fixtures |
| 4 | Replay + paper/shadow + charts | Determinism, fee/slippage model, historical annotation consistency |
| 5 | Regime/Trend/Structure/Volatility then volume/liquidity/flow | Availability-aware outputs and ablation tests |
| 6 | Modes, Microstructure/Burst/Exit/Flash | Measured latency distributions and replay equivalence |
| 7 | Learning/ONNX/model registry | Reproducible training, calibration, champion rollback |
| 8 | News/Macro/Correlation and Meta | No lookahead, proven source availability, cost-aware validation |
| 9 | Capsules, full backup/restore, multi-account, migration | Round-trip restore, schema/hash validation, secret exclusion |
| 10 | Extended demo validation then optional live | Measured execution, recovery and risk invariants under failure |

No deadline or profitability guarantee is implied. Features are marked complete
only after their own tests and target-environment acceptance.

## Source references

- Slint Rust integration: https://docs.slint.dev/latest/docs/rust/slint/
- MT5 initialize (starts terminal if necessary): https://www.mql5.com/en/docs/python_metatrader5/mt5initialize_py
- MT5 Python integration: https://www.mql5.com/en/docs/python_metatrader5
- Telegram Bot API: https://core.telegram.org/bots/api


## 0.1.1 packaging update

The portable preview contains a cross-compiled Windows x64 BrainTrade.exe and an
isolated CPython 3.13.16 runtime under runtime/python/. The core launches the
bundled script with that runtime, with no user Python/Rust installation. Source,
dependency notices, asset hashes and a PE import report accompany the preview.
Windows execution/MT5 acceptance is still pending; execution remains disabled.
