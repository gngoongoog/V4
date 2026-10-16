# BrainTrade OS — Current architecture and future boundaries

This document separates implemented capability from future research/release work. GitHub normal files/folders are the canonical source.

## 1. Process ownership

`BrainTrade.exe` is the desktop entry point and ensures one independent Core process. Core owns authenticated loopback IPC, persistence, gateway supervision, research services and Telegram. Closing the UI can leave Core running; Full Exit stops Core/gateway without touching MT5 positions.

The Python gateway collects account/positions/orders, quotes, completed bars/history and broker Market Book. The separately journaled execution adapter can submit bound-account orders after Core authorization; see `EXECUTION_AND_ROLES.md`. Imported ML models remain research-only.

## 2. Layering

- `crates/domain` — deterministic market/research/backtest/evaluation contracts and pure analytical logic.
- `crates/runtime` — Core lifecycle, SQLite, Shadow, historical services, chronological evaluation, learning registry/review/inference/scoring, execution coordination, portfolio exposure, news, backup and Telegram.
- `crates/desktop` — native Slint UI and Core API client; no broker execution ownership.
- `gateway` — official MetaTrader5 Python bridge with collection and a confined journaled execution adapter.
- `scripts` / `tests` — trainer, packaging, smoke/browser/contract/integration checks.
- `docs` — implementation boundaries, validation evidence, acceptance and continuity.

## 3. Live market, coordinator and execution path

Completed-bar market state feeds the deterministic research baseline and observational/specialized brains. The broker coordinator has 14 identities with coordinator/advisor/SOLO/RETURNING membership. Four operating modes are available: Scalping, Swing, adaptive Expert and live-quote Sniper.

Global START/STOP is independent from brain membership. Durable broker intents are persisted before dispatch, authorized again before send, journaled outside application-backup rollback and reconciled after timeout/restart. Partial fills are cumulative without blind top-up/resend. Unknown intents remain scoped to the original account.

Portfolio Exposure v1 is a separate default-OFF Core gate across all BrainTrade identities. When enabled it limits new aggregate exposure by combined positions/unresolved opens, lots and estimated stop-risk percentage. Unknown aggregate stop risk fails closed for new entries only; exits, broker SL/TP, timed management and reconciliation continue.

Shadow remains a separate virtual research ledger and must not be confused with broker execution.

## 4. Historical research

The historical lab supports Meta v1/v2, CES and KTE over frozen broker M1 history and frozen simulation assumptions. Exact History/report ledgers persist for replay, experiments and exports. KTE carries partial targets and stop updates. Self-contained HTML provides offline trade review.

Chronological evaluation runs explicit holdout/rolling Meta windows against immutable saved source identity. It is an evidence service, not an optimizer or promotion mechanism.

## 5. Learning pipeline

Current pipeline:

`completed-bar snapshot -> exact delayed label -> deterministic dataset -> offline trainer -> immutable challenger registry -> chronological evidence -> manual review eligibility -> read-only inference -> exact matured-label outcome scoring`

Execution-member weighting is separate from imported-model research and learns only from persisted prospective member observations. Imported challengers have no champion activation, automatic promotion, live self-modification or broker-execution route.

## 6. Persistence and backup

SQLite uses WAL + synchronous FULL. Foreign keys are explicitly enabled and checked on startup. Runtime tables include audit/control state, research/evaluation/learning evidence, execution intents/deals/member observations and persisted portfolio-exposure settings.

Backup schema 12 validates exact members, version/schema, checksums/config/SQLite integrity; stages a source plus rollback snapshot; and applies before SQLite startup under the Core lock. STOP is required. Secrets are excluded. The separately durable gateway send journal is intentionally outside application restore rollback so restoring an older Core state cannot re-send an already journaled broker order. Restore never enables execution.

## 7. IPC/security boundary

Control IPC remains loopback-only with bearer authentication. Origin-bearing browser requests are rejected. Secrets such as Telegram token are local and Windows DPAPI-backed where applicable. Source/package paths defensively exclude secrets, runtime DB/logs, `.import`, generated dependency trees and archives.

Do not expose the current loopback service through port forwarding as a remote-control mechanism.

## 8. Data semantics

- Candle tick volume is a broker activity proxy, not exchange volume.
- MT5 CFD Market Book is broker-specific and not global gold order flow.
- News carries source/freshness context and must not create fabricated millisecond-event claims.
- Missing/stale market inputs withdraw evidence; they are not synthetically filled.
- Historical OHLC cannot reveal intrabar order; ambiguous stop/target cases use explicit conservative rules.
- Correlation research must align timestamps with explicit availability masks rather than forward-filling unavailable source intervals.

## 9. Validation architecture

`research.yml` validates domain/runtime Rust tests, Core build, Python contracts/integration, real-Core synthetic smoke, browser report behavior, native desktop compile and headless all-page rendering. In 0.7 development it runs on pull requests as well as `development`, so evidence can be tied to the exact candidate SHA.

`windows.yml` runs Linux integration checks plus a real `windows-latest` package build. Pull-request runs validate exact candidate heads; push runs cover `main` and `development`. Both workflows use concurrency cancellation so superseded PR heads do not consume validation capacity.

CI packaging and simulated MT5 integration are not native acceptance against the user's actual MT5/broker. Permanent project-contract tests protect repository hygiene, journaled broker order access, causal learning wiring, shared schemas, release/development evidence separation and the absence of imported-model promotion/execution routes.

## 10. Planned work — not current capability

- Native target-machine Windows + Demo MT5 acceptance and separately recorded evidence.
- Private Telegram delivery acceptance with local-only credentials.
- Larger real completed-bar out-of-sample evidence and ablation studies.
- Historical news/event replay with first-observed no-lookahead timestamps.
- Aligned multi-symbol correlation/portfolio research with explicit availability masks.
- True feed-supported order flow/volume profile only where the source actually provides those fields.
- Any correlation-driven execution policy only after research evidence and a separate reviewed execution design.
- Imported-model Champion activation remains intentionally absent unless a future separately reviewed policy changes that boundary.

## 11. Release/version boundary

Promoted release: **0.6.0**. Current development: **0.7.0 Development Preview**. Development Cargo, lockfile, desktop and Windows package versions must stay aligned. Use `docs/validation.json` for promoted-release evidence and `docs/development-validation.json` for the exact current-development validation boundary.

No architecture item or test result is a profitability, latency or win-rate guarantee.
