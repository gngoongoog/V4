# BrainTrade OS — Roadmap

GitHub `gngoongoog/V4` is canonical. Read `BUILD_HANDOFF.md`, `PROJECT_STATE.md`, `CONTINUE_HERE.md` and the release/development validation records before editing runtime execution code.

## Promoted 0.6.0 checkpoint

The promoted 0.6.0 line remains the release baseline. Its archived validation evidence stays immutable. It contains bound-account execution, 14 durable execution identities, coordinator/advisor/SOLO/RETURNING membership, four trading modes, adaptive weighting, backup/restore and the native dashboard. Startup/restart is STOPPED. CI is simulated-broker validation; native MT5 demo acceptance remains a separate target-machine gate.

## Current 0.7.0 Development Preview

The 0.7 development line has advanced beyond the older validation checkpoint recorded in `docs/development-validation.json`. Intelligence V3 was merged through PR #47 at `30cc06390603491b7396f8c0c60a61c3d8416306` after exact PR-head Research and Windows/Linux validation.

Implemented 0.7 engineering includes:

- Portfolio Exposure v1 with durable persistence and stopped-only editing.
- Sniper Fast Lane plus execution-only Fast Pulse.
- Native Sniper MT5 `OnTick` + `CopyTicks(..., COPY_TICKS_ALL)` bridge with authenticated local transport, quote-order recovery and fallback polling.
- Account-scoped commission profiles with Round-trip or Per-side basis.
- Configurable 1–3600 second Sniper maximum hold, frozen at entry.
- Simplified five-section native Control Center.
- Fifteen merged execution identities through Liquidity Sweep, with append-only broker magic ownership.
- Correlation/exposure research, chronological holdout/rolling OOS evaluation and feature-group ablation, all research-only.
- Read-only Windows/Demo execution acceptance collection with native-vs-poll P50/P95/P99 timing characterization.
- Causal official-news historical replay using append-only first-observed timestamps and no execution route.
- Exact closed Demo broker-outcome adaptive learning using booked profit, commission, swap and fee.
- Portable Brain Snapshot support.
- Intelligence V3 member weighting with reward/confidence, regime/session context and execution-history quality evidence.

PR #48 is the next focused execution addition. It appends a 16th identity, `trend_pullback`, with magic `410615` while preserving `generated=410613` and `liquidity=410614`. Its logic uses completed bars only: EMA20/EMA50 trend separation and slope, a controlled pullback toward the fast mean, preservation of the slow-mean structure, a confirmation candle, anti-chasing distance and structural stop geometry. Treat the identity as part of `development` only after PR #48 is merged and its exact-head gates are green.

The Sniper local engineering target remains <=20 ms from BrainTrade queue to the `order_send` handoff on an appropriate target machine. This is not a guaranteed broker acknowledgement, fill-latency, network-latency, profitability or HFT claim.

## Immediate engineering and acceptance work

1. **Complete PR #48 exact-head validation and merge only if green.** Both Research and Windows/Linux workflows are required.
2. **Run native Windows + real Demo MT5 acceptance.** Follow `docs/DEMO_ACCEPTANCE_AR.md` and `docs/ACCEPTANCE.md`; preserve separate native OnTick and fallback-poll samples.
3. **Measure before optimizing execution again.** If `queue_to_send_ms` P95/P99 misses the local target, inspect `core_pulse_ms`, `authorize_ms`, `journal_ms`, `other_pre_send_ms`, `pre_send_ms`, `send_ms` and `total_ms` before changing architecture.
4. **Accumulate genuine OOS evidence.** Keep correlation, ablation, news replay and imported ML observational until enough prospective evidence exists.
5. **Evaluate Trend Pullback as a hypothesis, not a claim.** Track its closed Demo outcomes, execution costs and regime/session behavior before raising its learned influence.
6. **Private Telegram acceptance.** Test only with credentials stored on the target machine.
7. **Repository policy.** Enable required checks/branch protection for `main` and preferably `development` if administration access becomes available.

## Research and execution boundaries

- Exact account identity, quote freshness, ownership, final Core authorization, durable journal and uncertain-send reconciliation remain mandatory.
- Existing broker magic IDs must never be remapped; new identities append only.
- Correlation/exposure research cannot directly change broker execution or Portfolio Exposure v1.
- Imported ML remains audit/research-only; there is no automatic Champion promotion or execution route.
- Official-news replay is observational and does not infer trade direction from headline wording.
- True order-flow or volume-profile claims require feeds that actually provide the necessary fields.

## Explicitly not complete

- Native broker acceptance on the user's actual Windows/MT5 Demo installation.
- Real native-vs-poll Sniper latency distributions from that machine.
- Live Telegram acceptance.
- Imported ML Champion activation or automatic promotion.
- Correlation-driven broker execution policy.
- Guaranteed capture of every upstream broker event.
- Any profitability, execution-quality, HFT-latency or fill guarantee.

A module is complete only when code, verification and state documentation agree. A CI PASS proves the software path under the tested boundary; it does not substitute for target-machine broker acceptance.