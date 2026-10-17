# BrainTrade OS — verified 0.7.0 Development Preview

Canonical repository: `gngoongoog/V4`. Current verified runtime/source tree is `036989ffb18820920085fffa4b0517ace6e9904e`, represented by current `development` merge `4cf36107c72a59154859a8abdff64db70d6df21a` and PR #35 head `49ceb5956f9ac23aecaa03a444fa72abfc4fcf72` with the same complete tree.

Research validation: PASS — run `39011639762`.

Windows build/package and Linux integration: PASS — run `39011639745`.

The verified source passed 82 domain Rust tests, 63 runtime Rust tests and 102 Python tests. One explicit public-network official-news test is intentionally ignored. Core, backup/restore, Studio, chronological evaluation, OOS/correlation research, feature-ablation contracts, offline reports, simulated MT5 execution integrations, native selector compilation, native desktop rendering and Windows portable packaging passed.

## Current runtime scope

The package contains 15 execution identities; Scalping, Swing, Expert and Sniper; coordinator/advisor/SOLO/RETURNING membership; Liquidity Sweep; account-scoped commissions; configurable Sniper holding; durable execution reconciliation; Portfolio Exposure v1; the execution-only Sniper Fast Pulse; and the simplified native Control Center grouped into five primary sections.

The Sniper native path uses MT5 `OnTick` as a wake signal plus `CopyTicks(..., COPY_TICKS_ALL)` to recover terminal-cached executable quote changes through an authenticated local named pipe. The Python 5 ms quote poll remains fallback. The MQL5 EA is transport-only and contains no broker order submission API.

Fast pulses do not rerun the learner/research stack per quote. They update only validated quote fields on cached reconciled state, run execution/portfolio logic and return prefetched broker work. Final authorization, account recheck and durable journal commit before `order_send` remain mandatory.

## Cost and Sniper controls

Commission can be scoped exactly to login/server/currency/symbol and entered as Round trip or Per side. Per-side values are normalized to round-trip cost. Sniper maximum hold is configurable from 1 to 3600 seconds and is frozen with the trade at entry.

The local Sniper target remains <=20 ms for `queue_to_send_ms` on a suitable Windows machine. This is an engineering target only. It does not include a guarantee of broker acknowledgement, fill, network latency, profitability or HFT behavior.

## Research scope

Aligned multi-symbol correlation/exposure research is implemented with exact timestamp intersections, explicit unavailable data, deterministic persistence/replay/export and no execution connection. Chronological holdout/rolling OOS infrastructure and feature-group correlation/ablation tooling are also implemented.

These tools are scientifically useful but do not substitute for genuine prospective observations. Enough matured labels, completed-bar observations and independent holdout evidence still need to accumulate before any stability or edge claim is justified.

## Demo acceptance

The Windows package includes a read-only Demo acceptance collector in `runtime\gateway\check_runtime.py`. Its `--demo-acceptance` mode reads stored execution results, rejects Live/mismatched contexts, separates `native` and `poll` sessions, writes cumulative JSON/CSV output and calculates P50/P95/P99 for local execution timings.

CI validates the collector's safety/statistics contracts but cannot perform native acceptance against the user's actual MT5 Demo account. Real broker retcodes, fees, partial fills and target-machine latency therefore remain pending.

## Evidence and delivery

`docs/development-validation.json` records this development checkpoint and artifact hashes. The promoted 0.6 history in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` remains unchanged.

Current validation artifacts:

- Research artifact `12092852855` — `synthetic-research-validation` — SHA-256 `91b6e0b8273a60e90bd3c636fd55a66ffb7dee8b68e042399c8e623aa531da87`.
- Windows artifact `12093341260` — `BrainTrade-Windows-x64-Preview` — SHA-256 `6f85ae1562130da5da4c85bf0bff0d9cc07c88b6d4e84e89f3cb43137b3bd7da`.
- Native validation artifact `12093301318` — `windows-native-validation` — SHA-256 `6185b58db343063187aa97ae1d2f5e099e168d12e187652f00e39e064498a314`.

## Next steps

1. Perform real target-machine Windows + Demo MT5 acceptance using `docs/DEMO_ACCEPTANCE_AR.md` and `docs/ACCEPTANCE.md`.
2. Collect separate native and polling Sniper samples and preserve P50/P95/P99 evidence.
3. If queue-to-send misses the local target, fix the dominant measured component without removing durable correctness boundaries.
4. Continue genuine OOS accumulation and use correlation/ablation results as research evidence only.
5. Add first-observed historical news/event replay and causal joins.
6. Test private Telegram reports using credentials stored only on the target machine.
7. Enable branch protection/required checks if repository administration access becomes available.

Startup/restart is STOPPED. SOLO never starts global execution automatically. STOP blocks new entries while owned timed exits continue with Core active; Full Exit stops client-side management while broker-attached SL/TP remain with the broker. Imported ML challengers remain research-only with no automatic Champion promotion. CI does not establish profitability, guaranteed latency or broker execution quality.
