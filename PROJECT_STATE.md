# BrainTrade OS — project state: verified 0.7.0 Development Preview

Canonical repository: `gngoongoog/V4`. Current validated runtime/source checkpoint on `development`: PR #50 squash merge `ae5cc5c5bf216fc8d2453b204dbc45abe2e89902` (tree `db356f1eb5ae413543886f7a4f1427c883fad0da`). Exact PR head `3ff9425e2795102c8d84ea991260d985f2d4baba` has the same tree and passed Research plus Windows/Linux validation before merge.

Current exact-head CI scale is 87 domain Rust tests, 71 runtime Rust tests plus one intentionally ignored public-network news test, and 116 Python tests. Core, backup/restore, Studio, execution integration, offline reports, native desktop compilation/rendering, Linux validation and Windows portable packaging passed. Native MT5 Demo acceptance on the user's actual terminal remains a separate target-machine gate.

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

The Windows package contains the read-only collector at `runtime\gateway\check_runtime.py` plus the one-click operator introduced by PR #50:

- `Run-Demo-Acceptance.cmd`
- `Tools\Run-Demo-Acceptance.ps1`
- `DEMO_ACCEPTANCE_AR.md`

The operator runs an embedded runtime import preflight, launches the GET-only collector, and prints acceptance result, Sniper sample counts, `queue_to_send` P95/P99, `send_ms` P95, the largest local pre-send hotspot and Native-vs-Poll P95 comparison when both controlled sample sets exist. It does not enable START, change settings, bind an account or submit orders.

Windows CI validates PowerShell syntax before packaging and validates the packaged copy after staging. This validates the launcher/package contract only. CI has not run the collector against the user's actual Windows + MT5 Demo installation, so real fills/rejections/fees/partial fills and native-vs-poll latency distributions remain unverified.

## Evidence and delivery

`docs/development-validation.json` records the exact PR #50 checkpoint. The promoted 0.6 history in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` remains unchanged.

Research run `39050497842` passed on head `3ff9425e2795102c8d84ea991260d985f2d4baba` and uploaded `synthetic-research-validation` artifact `12106853812` (SHA-256 `fcb751f647d692f9da59c38f8b10eaa7233ece74f4c3901dcc370c51c5a0ebac`). Windows/Linux run `39050497836` passed on the same head and uploaded `BrainTrade-Windows-x64-Preview` artifact `12107094155` (SHA-256 `84f79393926209fc92d9eb3105bcae7ee5acc8ccd30ac41d97e0f37ff74fcf6d`) plus `windows-native-validation` artifact `12106889676` (SHA-256 `25b2be406bf64e7a8c5b4a05e8129d2cf5a854f3282a28bc51d7f18dcf151180`).

## Next steps

1. Run the packaged `Run-Demo-Acceptance.cmd` on the intended Windows + MT5 Demo machine and collect separate Native and Poll sessions.
2. If local P95/P99 misses the 20 ms queue-to-send target, optimize only the dominant measured component.
3. Accumulate genuine Trend Pullback/coordinator Demo and OOS evidence, including spread, fees, slippage and regime/session behavior.
4. Keep causal research lanes observational until independent evidence supports any promotion design.
5. Test private Telegram delivery with credentials stored only on the target machine.
6. Enable required branch checks when repository administration access is available.

Startup/restart remains STOPPED. SOLO never starts global execution automatically. CI does not establish profitability, guaranteed latency, broker execution quality or real native MT5 acceptance.
