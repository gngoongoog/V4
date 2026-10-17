# BrainTrade OS — Roadmap

GitHub `gngoongoog/V4` is canonical. Read `BUILD_HANDOFF.md`, `PROJECT_STATE.md`, `CONTINUE_HERE.md` and the release/development validation records before editing runtime execution code.

## Promoted 0.6.0 checkpoint

The promoted 0.6.0 line remains the release baseline. Its archived validation evidence stays immutable. It contains bound-account execution, 14 durable execution identities, coordinator/advisor/SOLO/RETURNING membership, four trading modes, adaptive weighting, backup/restore and the native dashboard. Startup/restart is STOPPED. CI is simulated-broker validation; native MT5 demo acceptance remains a separate target-machine gate.

## Current 0.7.0 Development Preview

PR #48 (`Add causal Trend Pullback execution brain`) is merged into `development` as `09c13460cee0a3fe42b4805ce1545529963783c7`. Exact PR head `9c08109e6414132c0ddd07588bb55d3550d759be` passed Research run `39045218522` and Windows/Linux run `39045218508`; both the tested head and merged commit use tree `7d952015f07c6022455500c12f321409cdc556c0`.

Implemented 0.7 engineering includes:

- Portfolio Exposure v1 with durable persistence and stopped-only editing.
- Sniper Fast Lane plus execution-only Fast Pulse.
- Native Sniper MT5 `OnTick` + `CopyTicks(..., COPY_TICKS_ALL)` bridge with authenticated local transport, quote-order recovery and fallback polling.
- Account-scoped commission profiles with Round-trip or Per-side basis.
- Configurable 1–3600 second Sniper maximum hold, frozen at entry.
- Simplified five-section native Control Center.
- Sixteen merged execution identities through Trend Pullback, with append-only broker magic ownership.
- Correlation/exposure research, chronological holdout/rolling OOS evaluation and feature-group ablation, all research-only.
- Read-only Windows/Demo execution acceptance collection with native-vs-poll P50/P95/P99 timing characterization.
- Causal official-news historical replay using append-only first-observed timestamps and no execution route.
- Exact closed Demo broker-outcome adaptive learning using booked profit, commission, swap and fee.
- Portable Brain Snapshot support.
- Intelligence V3 member weighting with reward/confidence, regime/session context and execution-history quality evidence.
- Trend Pullback with magic `410615`, preserving `generated=410613` and `liquidity=410614`. It uses completed bars only: EMA20/EMA50 trend separation and slope, controlled pullback toward EMA20, EMA50 structure preservation, confirmation quality, anti-chasing distance and structural stop geometry.

The Sniper local engineering target remains <=20 ms from BrainTrade queue to the `order_send` handoff on an appropriate target machine. This is not a guaranteed broker acknowledgement, fill-latency, network-latency, profitability or HFT claim.

## Immediate engineering and acceptance work

1. **Run native Windows + real Demo MT5 acceptance.** Follow `docs/DEMO_ACCEPTANCE_AR.md` and `docs/ACCEPTANCE.md`; preserve separate native OnTick and fallback-poll samples.
2. **Measure before optimizing execution again.** If `queue_to_send_ms` P95/P99 misses the local target, inspect `core_pulse_ms`, `authorize_ms`, `journal_ms`, `other_pre_send_ms`, `pre_send_ms`, `send_ms` and `total_ms` before changing architecture.
3. **Evaluate Trend Pullback prospectively.** Track real Demo closed outcomes, spread/fees/slippage and regime/session behavior before increasing learned influence or changing thresholds.
4. **Accumulate genuine OOS evidence.** Keep correlation, ablation, news replay and imported ML observational until enough prospective evidence exists.
5. **Private Telegram acceptance.** Test only with credentials stored on the target machine.
6. **Repository policy.** Enable required checks/branch protection for `main` and preferably `development` if administration access becomes available.

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