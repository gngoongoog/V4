# BrainTrade OS — verified 0.7.0 Development Preview

Canonical repository: `gngoongoog/V4`. Current validated runtime/source tree is `58a250ddca283e867436953a459959c33ee8a1e2`, represented by `development` merge `9524367f039b8fe59247e88c714c1cbff9c4a5a9` and PR #38 head `b6c15e8243cbf6792b88a9ca7bac3330643b619f` with the same runtime/source tree.

Research validation: PASS — run `39017294919`.

Windows build/package and Linux integration: PASS — run `39017294905`.

The verified source passed 82 domain Rust tests, 68 runtime Rust tests and 109 Python tests. One explicit public-network official-news test is intentionally ignored. Core, backup/restore, Studio, chronological evaluation, OOS/correlation research, feature-ablation contracts, offline reports, simulated MT5 execution integrations, native selector compilation, native desktop rendering and Windows portable packaging passed.

## Current runtime scope

The package contains 15 execution identities; Scalping, Swing, Expert and Sniper; coordinator/advisor/SOLO/RETURNING membership; Liquidity Sweep; account-scoped commissions; configurable Sniper holding; durable execution reconciliation; Portfolio Exposure v1; the execution-only Sniper Fast Pulse; and the simplified native Control Center grouped into five primary sections.

The Sniper native path uses MT5 `OnTick` as a wake signal plus `CopyTicks(..., COPY_TICKS_ALL)` to recover terminal-cached executable quote changes through an authenticated local named pipe. The Python 5 ms quote poll remains fallback. The MQL5 EA is transport-only and contains no broker order submission API.

Fast pulses do not rerun the learner/research stack per quote. They update only validated quote fields on cached reconciled state, run execution/portfolio logic and return prefetched broker work. Final authorization, account recheck and durable journal commit before `order_send` remain mandatory.

## Cost and Sniper controls

Commission can be scoped exactly to login/server/currency/symbol and entered as Round trip or Per side. Per-side values are normalized to round-trip cost. Sniper maximum hold is configurable from 1 to 3600 seconds and is frozen with the trade at entry.

The local Sniper target remains <=20 ms for `queue_to_send_ms` on a suitable Windows machine. This is an engineering target only. It does not include a guarantee of broker acknowledgement, fill, network latency, profitability or HFT behavior.

## Research scope

Aligned multi-symbol correlation/exposure research is implemented with exact timestamp intersections, explicit unavailable data, deterministic persistence/replay/export and no execution connection. Chronological holdout/rolling OOS infrastructure and feature-group correlation/ablation tooling are also implemented. PR #37 added causal official-news historical replay driven by append-only BrainTrade first-observed timestamps, with no forward fill across missing M1 bars and no execution route.

These tools are scientifically useful but do not substitute for genuine prospective observations. Enough matured labels, completed-bar observations and independent holdout evidence still need to accumulate before any stability or edge claim is justified.

## Real Demo adaptive learning

PR #38 makes adaptive coordinator learning consume exact closed MT5 Demo broker outcomes when available. A matched coordinator trade uses broker-reported profit, commission, swap and fee; only members that were ready and supported the executed direction receive the evidence. Broker evidence is preferred immediately, while the causal completed-bar path remains available as fallback.

This is real Demo feedback, not a Shadow-only simulation. Execution still requires explicit global START after restart and exact Demo account binding. Optional portfolio/per-brain risk caps default OFF, while identity, freshness, ownership, authorization, durable journal and duplicate/uncertain-send reconciliation remain mandatory.

## Demo acceptance

The Windows package includes a read-only Demo acceptance collector in `runtime\gateway\check_runtime.py`. Its `--demo-acceptance` mode reads stored execution results, rejects Live/mismatched contexts, separates `native` and `poll` sessions, writes cumulative JSON/CSV output and calculates P50/P95/P99 for local execution timings.

CI validates the collector's safety/statistics contracts but cannot perform native acceptance against the user's actual MT5 Demo account. Real broker retcodes, fees, partial fills and target-machine latency therefore remain pending.

## Evidence and delivery

`docs/development-validation.json` records this development checkpoint and artifact hashes. The promoted 0.6 history in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` remains unchanged.

Current validation artifacts:

- Research artifact `12095079199` — `synthetic-research-validation` — SHA-256 `412902f1ec2418f8d34c898a91a5823050894a4e9a41b1daeed2c3af3919c77f`.
- Windows artifact `12095677572` — `BrainTrade-Windows-x64-Preview` — SHA-256 `10c4b0730691866129b3848940f96ab5ad4be1019d04e0d99cffd1d86ea2c8ba`.
- Native validation artifact `12095269571` — `windows-native-validation` — SHA-256 `b764424d4d0cef36f03aba3000992a84b1ecfb62f5b539a8f121e9b61b9271fd`.

## Next steps

1. Perform real target-machine Windows + Demo MT5 acceptance using `docs/DEMO_ACCEPTANCE_AR.md` and `docs/ACCEPTANCE.md`.
2. Collect separate native and polling Sniper samples and preserve P50/P95/P99 evidence.
3. If queue-to-send misses the local target, fix the dominant measured component without removing durable correctness boundaries.
4. Continue genuine OOS accumulation and use correlation/ablation results as research evidence only.
5. Continue first-observed causal news/event research without connecting it to execution unless separately validated.
6. Test private Telegram reports using credentials stored only on the target machine.
7. Enable branch protection/required checks if repository administration access becomes available.

Startup/restart is STOPPED. SOLO never starts global execution automatically. STOP blocks new entries while owned timed exits continue with Core active; Full Exit stops client-side management while broker-attached SL/TP remain with the broker. Imported ML challengers remain research-only with no automatic Champion promotion. CI does not establish profitability, guaranteed latency or broker execution quality.
