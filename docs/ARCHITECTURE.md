# BrainTrade OS — Current architecture and future boundaries

This document separates what is implemented now from later execution/release work. GitHub normal files/folders are the canonical source.

## 1. Process ownership

`BrainTrade.exe` is the desktop entry point and ensures one independent Core process. The Core owns authenticated loopback IPC, persistence, gateway supervision, research services and Telegram. Closing the UI can leave Core running; Full Exit stops Core/gateway without touching MT5 positions.

The current Python MT5 gateway is deliberately **read-only**. It supplies account/positions/orders, quotes, completed bars/history and broker Market Book where available. Repository contract tests reject broker-write APIs.

## 2. Layering

- `crates/domain` — deterministic market/research/backtest/evaluation contracts and pure analytical logic.
- `crates/runtime` — Core lifecycle, SQLite, Shadow, historical services, chronological evaluation, learning registry/review/inference/scoring, news, backup and Telegram.
- `crates/desktop` — native Slint UI and Core API client; no broker execution ownership.
- `gateway` — official MetaTrader5 Python bridge with read-only collection.
- `scripts` / `tests` — trainer, packaging, smoke/browser/contract/integration checks.
- `docs` — implementation boundaries, acceptance and continuity.

## 3. Live market/research path

Completed-bar market state feeds the legacy deterministic Meta/Shadow baseline and observational brains: Trend, Regime, Structure, Volatility, Meta v2, Gann, zones/setups, broker DOM Depth and Macro/News context. Market Opportunities can expose research plans with structural stops/targets and cost/lot gates.

Only legacy Meta may create live **virtual** Shadow entries. Observational/new learning outputs cannot alter Shadow decisions or broker state.

## 4. Historical research

The historical lab supports Meta v1/v2, CES and KTE over frozen broker M1 history and frozen simulation assumptions. Exact History/report ledgers persist for replay, experiments and exports. KTE carries partial fills, targets and stop updates. Self-contained HTML provides offline trade review.

Chronological evaluation runs explicit holdout/rolling Meta windows against immutable saved source identity. It is an evidence service, not an optimizer or promotion mechanism.

## 5. Learning pipeline

Current pipeline:

`completed-bar snapshot -> exact delayed label -> deterministic dataset -> offline trainer -> immutable challenger registry -> chronological evidence -> manual review eligibility -> read-only inference -> exact matured-label outcome scoring`

The feature, label and model schemas are versioned. Inference consumes the same stored feature payload used for dataset export. Scoring stores immutable prediction outcomes and descriptive metrics.

There is no champion activation, auto-promotion, live self-modification or learned execution path.

## 6. Persistence and backup

SQLite uses WAL + synchronous FULL. Foreign keys are explicitly enabled and checked on startup. Runtime tables include audit/control state, market research state, historical reports/history, chronological evidence and learning audit records.

Backup schema 8 includes the implemented research/learning audit state and excludes secrets. Full restore/staged rollback remains future work and backups must not imply automatic execution on restore.

## 7. IPC/security boundary

Control IPC remains loopback-only with bearer authentication. Origin-bearing browser requests are rejected. Secrets such as Telegram token are local and Windows DPAPI-backed where applicable. Source/package paths defensively exclude secrets, runtime DB/logs, `.import`, generated dependency trees and archives.

Do not expose the current loopback service through port forwarding as a remote-control mechanism.

## 8. Data semantics

- Candle tick volume is a broker activity proxy, not exchange volume.
- MT5 CFD Market Book is broker-specific and not global gold order flow.
- News carries source/freshness context and must not create fabricated millisecond-event claims.
- Missing/stale market inputs withdraw evidence; they are not synthetically filled.
- Historical OHLC cannot reveal intrabar order; ambiguous stop/target cases use explicit conservative rules.

## 9. Validation architecture

`research.yml` validates domain/runtime Rust tests, Core build, Python tests, real-Core synthetic smoke, browser report behavior, native desktop compile and headless all-page rendering on `development`.

`windows.yml` validates Linux duplicates plus a real `windows-latest` package build on `main` and `development`. CI packaging is not native acceptance against the user's actual MT5/broker.

Permanent project-contract tests protect repository hygiene, read-only broker access, causal learning wiring, shared schemas and the absence of learned promotion/execution routes.

## 10. Planned work — not current capability

- Manual challenger performance review over sufficiently large scored outcomes.
- Broader real-data walk-forward/ablation research and later complex offline-trained model packaging only if justified.
- Historical news/event replay with no-lookahead observability timestamps.
- True feed-supported order flow/volume profile and aligned correlation/portfolio research.
- Full backup restore/migration.
- Broker execution state machine, hard execution risk, reconciliation, demo acceptance and only then any optional live mode.

Execution must persist intent before external side effects, treat send timeouts as unknown rather than blindly retry, reconcile broker state after restart and respect broker-discovered contract/session/account-mode constraints.

## 11. Release/version boundary

The research milestone is called 0.4.0 development while Cargo/Windows preview metadata remain 0.3.0. Normalize those versions only in an explicit release step with `Cargo.lock` and Windows validation. The last declared delivered portable remains 0.2.0 Preview.

No architecture item or test result is a profitability, latency or win-rate guarantee.
