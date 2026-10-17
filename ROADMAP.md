# BrainTrade OS — Roadmap

GitHub `gngoongoog/V4` is canonical. Read `BUILD_HANDOFF.md`, `PROJECT_STATE.md`, `CONTINUE_HERE.md` and the release/development validation records before editing runtime execution code.

## Promoted 0.6.0 checkpoint

The promoted 0.6.0 line remains the release baseline. Its archived validation evidence stays immutable. Startup/restart is STOPPED. CI is simulated-broker validation; native MT5 Demo acceptance remains a separate target-machine gate.

## Current 0.7.0 Development Preview

PR #50 (`Add one-click MT5 Demo acceptance operator`) is merged into `development` as `ae5cc5c5bf216fc8d2453b204dbc45abe2e89902`. Exact tested head `3ff9425e2795102c8d84ea991260d985f2d4baba` passed Research run `39050497842` and Windows/Linux run `39050497836`; both tested head and merge use tree `db356f1eb5ae413543886f7a4f1427c883fad0da`.

Implemented 0.7 engineering includes:

- Portfolio Exposure v1 with durable persistence and stopped-only editing.
- Sniper Fast Lane plus execution-only Fast Pulse.
- Native Sniper MT5 `OnTick` + `CopyTicks(..., COPY_TICKS_ALL)` bridge with authenticated local transport and fallback polling.
- Account-scoped commission profiles with Round-trip or Per-side basis.
- Configurable 1–3600 second Sniper maximum hold, frozen at entry.
- Simplified five-section native Control Center.
- Sixteen merged execution identities through Trend Pullback with append-only broker magic ownership.
- Correlation/exposure research, chronological holdout/rolling OOS evaluation and feature-group ablation, all research-only.
- Causal official-news historical replay with no execution route.
- Exact closed Demo broker-outcome adaptive learning and Intelligence V3 member weighting.
- Portable Brain Snapshot support.
- Read-only Windows/Demo execution acceptance collector with native-vs-poll P50/P95/P99 timing characterization.
- One-click packaged acceptance operator (`Run-Demo-Acceptance.cmd` + `Tools\Run-Demo-Acceptance.ps1`) with runtime preflight, acceptance summary and Native-vs-Poll P95 comparison.
- Windows PowerShell parser validation of the acceptance operator source and packaged copy.

The Sniper local engineering target remains <=20 ms from BrainTrade queue to the `order_send` handoff on an appropriate target machine. This is not a guaranteed broker acknowledgement, fill latency, network latency, profitability or HFT claim.

## Immediate engineering and acceptance work

1. **Run native Windows + real Demo MT5 acceptance.** Use packaged `Run-Demo-Acceptance.cmd`; preserve separate Native OnTick and fallback-poll samples.
2. **Measure before optimizing execution again.** If `queue_to_send_ms` P95/P99 misses the local target, inspect `core_pulse_ms`, `authorize_ms`, `journal_ms`, `other_pre_send_ms`, `pre_send_ms`, `send_ms` and `total_ms` before changing architecture.
3. **Evaluate Trend Pullback prospectively.** Track real Demo closed outcomes, spread/fees/slippage and regime/session behavior before changing thresholds or learned influence.
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
