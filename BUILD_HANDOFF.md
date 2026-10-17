# BrainTrade OS — verified 0.7.0 Development Preview

GitHub `gngoongoog/V4` is canonical. Continue from `development`; use `main` only when preparing a promoted release.

## Current verified checkpoint

PR #50 (`Add one-click MT5 Demo acceptance operator`) is merged into `development` as `ae5cc5c5bf216fc8d2453b204dbc45abe2e89902`. Its exact tested head `3ff9425e2795102c8d84ea991260d985f2d4baba` passed Research run `39050497842` and Windows/Linux run `39050497836`. The tested head and squash merge share tree `db356f1eb5ae413543886f7a4f1427c883fad0da`.

Current verified scale is 87 domain Rust tests, 71 runtime Rust tests, one intentionally ignored public-network official-news test, and 116 Python tests. Core, backup/restore, Studio, execution integration, offline reports, native selector compilation/rendering, Linux validation and Windows portable packaging passed.

Promoted 0.6.0 evidence remains immutable and separate. This 0.7 checkpoint is a Development Preview, not a promoted production release.

## Implemented runtime and execution

- 16 append-only execution identities with coordinator/advisor/SOLO/RETURNING roles.
- Broker ownership remains stable: `generated=410613`, `liquidity=410614`, `trend_pullback=410615`.
- Scalping, Swing, Expert and Sniper modes.
- Durable broker intent/reconciliation, split-fill handling, account isolation, no blind resend after uncertain results and mandatory final authorization before send.
- Portfolio Exposure v1, account-scoped commissions and configurable frozen Sniper hold.
- Sniper Fast Lane and execution-only `PUT /v1/execution/pulse`.
- Native Windows MT5 bridge using `OnTick` plus `CopyTicks(..., COPY_TICKS_ALL)`, authenticated local transport, bounded FIFO and automatic 5 ms Python polling fallback.
- Trend Pullback uses completed bars only with EMA20/EMA50 trend structure, controlled pullback, confirmation quality, anti-chasing and structural stop geometry.
- Exact closed Demo broker-outcome learning using booked `profit + commission + swap + fee` for participating coordinator members.
- Intelligence V3 weighting using reward/confidence, regime/session context and execution-quality evidence.
- Native five-section Control Center and persisted runtime state.

## Research boundary

Historical Meta v1/v2, CES v1, KTE v1, Wyckoff, trend/regime/structure/volatility/Gann/liquidity analysis, aligned correlation, chronological holdout/rolling OOS, feature-group ablation, causal official-news replay, snapshots/labels/export and imported challengers are implemented as research/evidence lanes.

Correlation, ablation, news replay and imported ML do not directly authorize broker execution. Imported ML has no automatic Champion promotion route.

## Demo acceptance tooling

The packaged Windows preview now contains:

- `Run-Demo-Acceptance.cmd` at the package root.
- `Tools\Run-Demo-Acceptance.ps1` for interactive or scripted operation.
- `DEMO_ACCEPTANCE_AR.md` at the package root.
- `runtime\gateway\check_runtime.py` as the underlying read-only collector.

The operator performs an embedded Python/NumPy/MetaTrader5 import preflight, launches the read-only collector, summarizes `queue_to_send` P95/P99, `send_ms` P95, the largest local pre-send hotspot, and Native-vs-Poll sample counts/P95. It never enables START, binds an account, changes settings or submits an order.

Windows CI parses the PowerShell source before packaging and parses the packaged copy again after staging. This proves syntax/package wiring only; it does **not** perform real MT5 Demo acceptance.

## Sniper latency boundary

The engineering target remains **<=20 ms for BrainTrade-local `queue_to_send_ms`** on the intended Windows machine. Use `core_pulse_ms`, `queue_to_gateway_ms`, `authorize_ms`, `journal_ms`, `other_pre_send_ms`, `pre_send_ms`, `queue_to_send_ms`, `send_ms` and `total_ms` to locate the measured bottleneck.

Do not remove account identity, quote freshness, ownership, authorization, durable journal or uncertain-send reconciliation merely to improve a benchmark. `send_ms` is MT5/network/broker dependent and is not part of the local <=20 ms claim.

## Exact CI evidence

Research run `39050497842`: SUCCESS.

- Artifact `12106853812` — `synthetic-research-validation`
- Size: `4308194` bytes
- SHA-256: `fcb751f647d692f9da59c38f8b10eaa7233ece74f4c3901dcc370c51c5a0ebac`

Windows/Linux run `39050497836`: SUCCESS.

- Artifact `12107094155` — `BrainTrade-Windows-x64-Preview`
  - Size: `34549437` bytes
  - SHA-256: `84f79393926209fc92d9eb3105bcae7ee5acc8ccd30ac41d97e0f37ff74fcf6d`
- Artifact `12106889676` — `windows-native-validation`
  - Size: `1704761` bytes
  - SHA-256: `25b2be406bf64e7a8c5b4a05e8129d2cf5a854f3282a28bc51d7f18dcf151180`

`docs/development-validation.json` is the machine-readable 0.7 development evidence record. Promoted 0.6 evidence in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` remains immutable historical evidence.

## Target-machine work still pending

1. On the intended Windows PC, open the packaged `BrainTrade.exe`, connect the intended MT5 **Demo** account and follow `DEMO_ACCEPTANCE_AR.md` / `docs/ACCEPTANCE.md`.
2. Collect a controlled Native session with `BrainTradeSniperBridge` attached and a separate Poll session with it detached/stopped.
3. Preserve the generated acceptance JSON/CSV and evaluate real retcodes, fees, rejections, partial fills and P50/P95/P99 timing distributions.
4. If local latency misses the target, optimize only the measured dominant component.
5. Accumulate genuine prospective/OOS evidence for Trend Pullback and the coordinator before changing influence or thresholds.
6. Test private Telegram delivery with credentials stored only on the target machine.
7. Enable required branch checks when repository administration access is available.

Startup/restart remains STOPPED. CI does not establish profitability, broker fill quality, guaranteed latency or native MT5 acceptance. The Demo acceptance collector remains read-only and must not manufacture trades.
