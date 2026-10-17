# BrainTrade OS — Roadmap

GitHub `gngoongoog/V4` is canonical. Read `BUILD_HANDOFF.md`, `PROJECT_STATE.md`, `CONTINUE_HERE.md` and both release/development validation records before editing.

## Promoted 0.6.0 checkpoint

Verified executable/source checkpoint: `fdc63d4d05df89e27607641bd197b9e6ead936e9` (tree `8593783f43a4892cdc724ed5d112607ef7cee006`). Research CI `38803539711`: PASS. Windows CI/package `38803539405`: PASS. PR #13 promoted the reviewed 0.6.0 line to `main` without history rewriting.

The promoted line contains bound-account execution, 14 execution identities, coordinator/advisor/SOLO/RETURNING membership, Scalping/Swing/Expert/Sniper, adaptive member weighting, schema-12 backup/restore and the earlier native dashboard. Startup/restart is STOPPED. CI uses simulated MT5; native MT5 demo acceptance remains a separate target-machine gate.

## Current 0.7.0 Development Preview

Current `development` runtime/source merge checkpoint is `4cf36107c72a59154859a8abdff64db70d6df21a` (tree `036989ffb18820920085fffa4b0517ace6e9904e`). PR #35 head `49ceb5956f9ac23aecaa03a444fa72abfc4fcf72` has the same tree and passed Research plus Windows/Linux validation before squash merge. Historical 0.6 release records remain unchanged.

Current CI evidence scale is 82 domain Rust tests, 63 runtime Rust tests plus one intentionally ignored public-network news test, and 102 Python tests. Core build, lifecycle, backup/restore, Studio, chronological evaluation, OOS/correlation research, feature ablation contracts, simulated broker execution, native selector compilation, desktop rendering and Windows portable packaging passed.

Completed 0.7 engineering now includes:

- Portfolio Exposure v1 with durable persistence and stopped-only editing.
- Sniper Fast Lane and the execution-only `PUT /v1/execution/pulse` path that reuses reconciled state instead of running research/learning on every quote.
- Native Sniper MT5 `OnTick` + `CopyTicks(..., COPY_TICKS_ALL)` bridge with authenticated local transport, source-order recovery, same-millisecond event separation and automatic 5 ms Python fallback.
- Account-scoped commission profiles with Round-trip or Per-side basis and exact account/server/currency/symbol matching.
- Configurable Sniper maximum hold time (1–3600 seconds), frozen at entry so an already-open position does not change when settings change.
- Simplified native Control Center grouped into five primary sections: الرئيسية، التداول، البحث، المختبر، النظام.
- Fifteen execution identities, including the Liquidity Sweep brain, without remapping earlier brain identities.
- Aligned multi-symbol correlation/exposure research with exact timestamp intersections, explicit availability handling, deterministic persistence/replay/export and no broker-policy connection.
- Chronological holdout/rolling OOS evaluation infrastructure and feature-group correlation/ablation tooling. These are research tools; real prospective evidence still has to accumulate.
- Read-only Windows/Demo execution acceptance collector that analyzes stored real execution results, separates `native` and `poll` sessions, and calculates P50/P95/P99 without creating trades itself.

The Sniper local engineering target is <=20 ms from BrainTrade queue to the `order_send` handoff on an appropriate target machine. The collector measures component timing and identifies whether delay is dominated by Core/pulse, authorization, durable journal, other pre-send MT5 work, or `order_send`. This is not a guaranteed MT5/broker acknowledgement, fill latency, network latency, profitability or HFT claim.

## Immediate engineering and acceptance work

1. **Run native Windows + real Demo MT5 acceptance.** Follow `docs/DEMO_ACCEPTANCE_AR.md` and `docs/ACCEPTANCE.md`. Collect at least a useful sample of real Sniper sends for both native OnTick and 5 ms fallback and preserve P50/P95/P99 reports.
2. **Measure before optimizing again.** If `queue_to_send_ms` P95/P99 misses the 20 ms local target, use `core_pulse_ms`, `authorize_ms`, `journal_ms`, `other_pre_send_ms`, `pre_send_ms`, `send_ms` and `total_ms` to identify the actual bottleneck before changing execution code.
3. **Accumulate genuine OOS evidence.** Keep holdout/rolling evaluation, feature ablation and correlation observational until enough completed-bar observations and matured labels exist.
4. **Historical event replay.** Add first-observed event timestamps and causal joins before any news/event signal can be tested historically.
5. **Private Telegram acceptance.** Test only with credentials stored on the target machine.
6. **Repository policy.** Enable required checks/branch protection for `main` and preferably `development` if repository administration access becomes available.

## Research boundaries that remain intentional

- Correlation/exposure results are research-only and cannot change broker execution or Portfolio Exposure v1.
- Imported ML models remain audit/research challengers; there is no automatic Champion promotion or execution route.
- Feature ablation can identify duplicate/weak groups but does not itself authorize removal from production logic without prospective evidence.
- Official-news ingestion is observational; headline wording does not infer direction and is not a trading signal.
- True order-flow/volume-profile claims require a feed that actually supplies the necessary fields.

## Explicitly not complete

- Native broker acceptance on the user's actual MT5 installation and Demo account.
- Real measured native-vs-poll Sniper latency distributions from that machine.
- Live Telegram acceptance.
- Imported ML Champion activation or automatic promotion.
- Correlation-driven broker execution policy.
- Historical causal news/event replay.
- Guaranteed capture of every upstream broker event.
- Any profitability, HFT-latency, execution-quality or fill guarantee.

A module is complete only when code, verification and state documentation agree. A CI PASS proves the software path under the tested boundary; it does not substitute for target-machine broker acceptance.
