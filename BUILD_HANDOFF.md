# BrainTrade OS — verified 0.7.0 Development Preview

GitHub `gngoongoog/V4` is canonical. Continue from `development`; use `main` only when preparing a promoted release.

## Current verified checkpoint

PR #53 (`Add privacy-safe Demo acceptance export bundle`) is merged into `development` as `b20af1f0a3ae37d30e688e92615972e5c03e5adc`. Its exact tested head `31c10767e7c284160a74c1e0b001c0fde825780d` passed Research run `39058307544` and Windows/Linux run `39058307570`. The tested head and merge share tree `cb4cfc79358305d91caa4826ab2d47414fbd745b`.

Current verified scale is 87 domain Rust tests, 71 runtime Rust tests, one intentionally ignored public-network official-news test, and 118 Python tests. Core, backup/restore, Studio, execution integration, offline reports, native selector compilation/rendering, Linux validation and Windows portable packaging passed.

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
- `Tools\Run-Demo-Acceptance-GUI.ps1` as the one-click Windows Demo Acceptance Center.
- `Tools\Run-Demo-Acceptance.ps1` for scripted/advanced operation.
- `DEMO_ACCEPTANCE_AR.md` at the package root.
- `runtime\gateway\check_runtime.py` as the underlying read-only collector.

The Acceptance Center can run controlled Native, Poll or Auto capture sessions and display acceptance result, Sniper sample count, `queue_to_send` P95/P99, `send_ms` P95, Native/Poll counts/P95 and the largest local pre-send hotspot. It never enables START, binds an account, changes execution settings or submits an order.

PR #53 adds a privacy-safe `تصدير حزمة مشاركة` path for target-machine evidence. The exported ZIP is allowlisted to acceptance JSON/CSV/state/runtime-import evidence plus a reduced summary/README. It intentionally excludes raw MT5 login, `ipc.key`, broker journals/databases, Telegram credentials and GUI stdout/stderr logs.

Windows CI parses both acceptance PowerShell files before packaging and parses the packaged copies again after staging. This proves syntax/package wiring only; it does **not** perform real MT5 Demo acceptance.

## Sniper latency boundary

The engineering target remains **<=20 ms for BrainTrade-local `queue_to_send_ms`** on the intended Windows machine. Use `core_pulse_ms`, `queue_to_gateway_ms`, `authorize_ms`, `journal_ms`, `other_pre_send_ms`, `pre_send_ms`, `queue_to_send_ms`, `send_ms` and `total_ms` to locate the measured bottleneck.

Do not remove account identity, quote freshness, ownership, authorization, durable journal or uncertain-send reconciliation merely to improve a benchmark. `send_ms` is MT5/network/broker dependent and is not part of the local <=20 ms claim.

## Exact CI evidence

Research run `39058307544`: SUCCESS on tested head `31c10767e7c284160a74c1e0b001c0fde825780d`.

- Artifact `12110491477` — `synthetic-research-validation`
- Size: `4309247` bytes
- SHA-256: `5a7b7c2dba278110f0c536aaa6cc162d3a509ed71da92137828dd0c10aaff5e9`

Windows/Linux run `39058307570`: SUCCESS on the same tested head.

- Artifact `12110404218` — `BrainTrade-Windows-x64-Preview`
  - Size: `34560194` bytes
  - SHA-256: `e7d90d7744264e30c1febf0fc835ee78a849901a303177f73f3261c45e6b0c27`
- Artifact `12111210477` — `windows-native-validation`
  - Size: `1704761` bytes
  - SHA-256: `05fa8cdbb70c35e6f04c084d0b28a8b034c8bb665832f21fc235bdb523aab99d`

`docs/development-validation.json` is the machine-readable 0.7 development evidence record. Promoted 0.6 evidence in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` remains immutable historical evidence.

## Target-machine work still pending

1. On the intended Windows PC, open the packaged `BrainTrade.exe`, connect the intended MT5 **Demo** account and open `Run-Demo-Acceptance.cmd`.
2. Collect a controlled Native session with `BrainTradeSniperBridge` attached and a separate Poll session with it detached/stopped.
3. Use `تصدير حزمة مشاركة` after capture so the JSON/CSV/state and reduced summary can be inspected without sending account secrets or broker databases.
4. Evaluate real retcodes, fees, rejections, partial fills and P50/P95/P99 timing distributions. If local latency misses the target, optimize only the measured dominant component.
5. Accumulate genuine prospective/OOS evidence for Trend Pullback and the coordinator before changing influence or thresholds.
6. Test private Telegram delivery with credentials stored only on the target machine.
7. Enable required branch checks when repository administration access is available.

Startup/restart remains STOPPED. CI does not establish profitability, broker fill quality, guaranteed latency or native MT5 acceptance. The Demo acceptance collector and Acceptance Center remain read-only and must not manufacture trades.
