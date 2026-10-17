# BrainTrade OS — project state: verified 0.7.0 Development Preview

Canonical repository: `gngoongoog/V4`. Current validated runtime/source checkpoint on `development`: PR #53 merge `b20af1f0a3ae37d30e688e92615972e5c03e5adc` (tree `cb4cfc79358305d91caa4826ab2d47414fbd745b`). Exact PR head `31c10767e7c284160a74c1e0b001c0fde825780d` has the same tree and passed Research plus Windows/Linux validation before merge.

Current exact-head CI scale is 87 domain Rust tests, 71 runtime Rust tests plus one intentionally ignored public-network news test, and 118 Python tests. Core, backup/restore, Studio, execution integration, offline reports, native desktop compilation/rendering, Linux validation and Windows portable packaging passed. Native MT5 Demo acceptance on the user's actual terminal remains a separate target-machine gate.

## Execution and Sniper state

The development package contains 16 durable execution identities. Broker ownership remains append-only: `generated=410613`, `liquidity=410614`, `trend_pullback=410615`. It includes Scalping, Swing, Expert and Sniper modes; coordinator/advisor/SOLO/RETURNING membership; durable intent/reconciliation; split-fill and uncertain-send recovery; Portfolio Exposure v1; account-scoped commissions; configurable Sniper hold; and the native Control Center.

Sniper has a native Windows MT5 `OnTick` + `CopyTicks(..., COPY_TICKS_ALL)` path and an automatic 5 ms Python polling fallback. Fast quotes use execution-only `PUT /v1/execution/pulse`; final `/v1/execution/authorize` remains mandatory. The Python adapter rechecks the account and preserves the durable `broker-journal.sqlite` commit-before-`order_send` boundary with no blind resend after uncertain outcomes.

The local engineering target is **<=20 ms for `queue_to_send_ms`** on a suitable target machine. It is not a broker acknowledgement, fill, network or profitability guarantee.

## Trend Pullback state

PR #48 added `trend_pullback` as identity 16 without remapping any prior Magic ID. It uses completed bars only: EMA20/EMA50 trend structure, a controlled pullback toward EMA20 while preserving EMA50 structure, a confirmation candle, anti-chasing limits and ATR-buffered structural stop geometry. CI proves source/execution contracts, not profitability.

## Research and adaptive-learning state

The development branch includes deterministic historical Meta v1/v2, CES v1 and KTE v1 research; Wyckoff; Trend/Regime/Structure/Volatility/Gann/Liquidity analysis; chronological evaluation; aligned multi-symbol correlation; holdout/rolling OOS; feature-group ablation; causal official-news replay; snapshots/labels/export; imported challenger inference; and Intelligence V3.

Correlation, ablation, news replay and imported ML remain research-only. Exact closed Demo broker outcomes feed adaptive coordinator evidence from broker-reported `profit + commission + swap + fee` for members that supported the executed side at entry.

## Demo acceptance state

The Windows package contains the read-only collector at `runtime\gateway\check_runtime.py` and the packaged operator stack:

- `Run-Demo-Acceptance.cmd`
- `Tools\Run-Demo-Acceptance-GUI.ps1`
- `Tools\Run-Demo-Acceptance.ps1`
- `DEMO_ACCEPTANCE_AR.md`

PR #52 added the Windows Demo Acceptance Center. Double-clicking the CMD opens the GUI, while command-line arguments preserve the CLI workflow. The GUI can run Native/Poll/Auto sessions and display acceptance result, Sniper sample counts, `queue_to_send` P95/P99, `send_ms` P95, the largest local pre-send hotspot and Native-vs-Poll P95 comparison.

PR #53 added a privacy-safe share export. The exporter builds an allowlisted ZIP containing available acceptance JSON/CSV/state/runtime-import evidence plus `share-summary.json` and `SHARE_README.txt`. It intentionally excludes raw MT5 login, `ipc.key`, broker journals/databases, Telegram credentials and GUI stdout/stderr logs. The GUI also terminates the collector process tree when the operator explicitly closes an active capture session; it still does not change BrainTrade START/STOP state.

The collector/operator stack does not enable START, change execution settings, bind an account or submit orders. Windows CI validates PowerShell syntax before packaging and validates the packaged copies after staging. CI has not run the collector against the user's actual Windows + MT5 Demo installation, so real fills/rejections/fees/partial fills and native-vs-poll latency distributions remain unverified.

## Evidence and delivery

`docs/development-validation.json` records the exact PR #53 checkpoint. The promoted 0.6 history in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` remains unchanged.

Research run `39058307544` passed on head `31c10767e7c284160a74c1e0b001c0fde825780d` and uploaded `synthetic-research-validation` artifact `12110491477` (SHA-256 `5a7b7c2dba278110f0c536aaa6cc162d3a509ed71da92137828dd0c10aaff5e9`). Windows/Linux run `39058307570` passed on the same head and uploaded `BrainTrade-Windows-x64-Preview` artifact `12110404218` (SHA-256 `e7d90d7744264e30c1febf0fc835ee78a849901a303177f73f3261c45e6b0c27`) plus `windows-native-validation` artifact `12111210477` (SHA-256 `05fa8cdbb70c35e6f04c084d0b28a8b034c8bb665832f21fc235bdb523aab99d`).

## Next steps

1. Run the packaged `Run-Demo-Acceptance.cmd` on the intended Windows + MT5 Demo machine and collect separate Native and Poll sessions.
2. Export the privacy-safe share bundle and inspect the measured local timing components and broker result states before changing execution architecture.
3. If local P95/P99 misses the 20 ms queue-to-send target, optimize only the dominant measured component.
4. Accumulate genuine Trend Pullback/coordinator Demo and OOS evidence, including spread, fees, slippage and regime/session behavior.
5. Keep causal research lanes observational until independent evidence supports any promotion design.
6. Test private Telegram delivery with credentials stored only on the target machine.
7. Enable required branch checks when repository administration access is available.

Startup/restart remains STOPPED. SOLO never starts global execution automatically. CI does not establish profitability, guaranteed latency, broker execution quality or real native MT5 acceptance.
