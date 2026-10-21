# P0 Stage 2 / v0.7.4 — isolated mandatory stop-risk lot cap

Risk-based sizing occurs at MT5 broker verification for each fresh Core entry, preserving requested lot as an upper bound. Uses actual broker order_calc_profit, account equity, configured fee and slippage estimates. This is not a tick-level profit model or a guarantee of maximum realized loss. Requires exact-head CI and native Demo tests before real-money use.

# Current 0.7.3 — P0 fee/lot-contract stage 1

Commission fee model for round-trip lot sizes is aligned across Core, Rust trading and risk, demo margin downsize, native preview and test suite. 7 per 1 lot -> 0.07 / 0.70 / 7.00 for 0.01 / 0.10 / 1.00; 0 means zero modeled broker commission only. Risk-sized lot based on stop is a future separately tested stage. Exact commit Research+Windows CI must pass before merging. See docs/P0_COMMISSION_STAGE_1_AR.md.

# Current 0.7.2 — gated REAL account confirmation

The 0.7.2 Core/UI must be paired (no 0.7.1 Core reuse). LIVE is not certified: every bind and start is separately acknowledged and checked against fresh MT5 broker identity and portfolio protection. This remains a demo-controlled review pending exact-head CI and real-machine acceptance. See `docs/REAL_ACCOUNT_SWITCH_ACCEPTANCE_AR.md`.

# Current 0.7.1 follow-up

PR #74 adds the brain control room, scoped participation/paper controls, four Sniper learner styles and Arabic Telegram. Continue from the latest main; older checkpoints below are historical. See `docs/BRAIN_CONTROL_SNIPER_AR.md` and exact-head CI. Native MT5 demo acceptance remains outstanding.

# BrainTrade OS — project state: verified 0.7.0 Development Preview


## Latest verified checkpoint — contextual mistake memory (2026-10-19)

PR #64 is merged into `development` as `88a341fc8733deaa1856079fc073bb744a61afcf`, source tree `59efba92fc67eb249336695074e9c67eafb7537b`. This includes PR #62 timing/five-way exits and PR #63 handoff documentation. Post-merge Research `39249735055` and Windows/Linux `39249735059` both passed on that exact merge.

Windows evidence: 103 domain, 85 runtime and 3 desktop Rust tests passed, with one public-network test ignored; 129 of 130 Python tests passed, with one OS-specific skip. Packaged startup/DPAPI/STOPPED default, all four modes, backup/restore/restart, execution integration and the $100 Flexible Demo/student integration passed. All 40 native frames cover 20 pages. Native MT5 acceptance, actual broker latency and profitability remain unverified.

The current learner reduces a ready Demo advisor's preference after at least three recent consecutive booked losses in the same strategy/market context. The penalty is capped at 0.20, clears with a newer non-loss or expiration of the 14-day window, and never vetoes a valid candidate. Exploration graduates from 30% toward 5% using total eligible contextual evidence while retaining novelty-weighted sampling. The entry journal shows the frozen streak and penalty. Five-way exits, legacy studies, research-only timing and 16 execution identities remain intact.

Windows handoff: `BrainTrade-Windows-Learning-88a341f.zip` (36,203,052 bytes), SHA256 `0f0ffb68f16f6bd04d41e740b1735cc2b477931542b8aae76190f9f9dba8c6bb`. It is the unchanged inner package from successful post-merge artifact `12186286037`. Read `docs/LATEST_WINDOWS_HANDOFF_AR.md` and `docs/latest-learning-validation.json`. Take an application backup before upgrading; older binaries cannot read new five-way study records. Native Demo acceptance is the next evidence step; automatic application of timing recommendations remains unimplemented. This delivery documentation changes no tested execution code.


## Previous merged development status — PR #62

PR #62 is merged into `development` as `29fe6663972651e8e3a1b613b9d957a86590bf6e`. Post-merge Research run `39240501418` and Windows/Linux run `39240501431` completed successfully. Preview artifact `12180999893` (`BrainTrade-Windows-x64-Preview`) is available from the Windows workflow; see `docs/pr62-learning-validation.json`.

The current learner keeps legacy three-way exit studies and adds new Baseline/Quick/Patient/Protective/Extended studies. It also observes independent quote-only prospective 0/250/1000 ms entry timings for queued Demo opportunities; no research-only timing experiment can create or delay an MT5 order. Actual settled PnL remains distinct from shadow estimates. 16 broker execution identities remain in the merged branch. The previous PR #55 evidence below is a **historical** audit checkpoint, not the current development HEAD.

Remaining: target-machine Windows + MT5 Demo acceptance, quote coverage and cost/latency evidence; review unmerged PR #58 for compatibility. Promoted `main` 0.6.0 is unchanged.

Canonical repository: `gngoongoog/V4`. Verified 0.7.0 `development` state.

PR #55 (comprehensive 0.7 correctness audit) is merged into `development` as `2e05cf3b907073c5faebe25fd16d4a4a523b224d`. Exact tested PR head `892807ec317c856b1215efb569e726554266709b` and the merge share code tree `a85fa0c424e23cf9e82790c649d25461e1547387`. Research run `39067458928` and Windows/Linux run `39067458927` both succeeded on that code.

Verified count: 87 domain Rust tests, 72 runtime Rust tests plus one intentionally ignored official-news public-network test, three desktop Rust tests, and 124 of 125 Windows Python tests passed (one OS-specific fixture skipped). The Windows CI executed the real named-pipe blocking-connect shutdown/rebind regression, built the portable preview and passed simulated execution integration. The Research CI passed browser report and complete native-page rendering. Real Windows/MT5 Demo acceptance remains pending.

PR #55 fixed native pipe shutdown/rebind and dead-worker detection, stopped-only execution-affecting Risk/Trading settings, side-effect-free rejection of invalid `/v1/config`, explicit-only START, final post-journal Core authorization and account recheck before `order_send`, safe timeframe indexing, accurate START audit metadata and complete Python compile checks. It adds `post_journal_authorize_ms` and `post_journal_account_ms` latency evidence without double counting.

Promoted 0.6.0 evidence remains immutable. BrainTrade 0.7.0 is a Development Preview, not production/broker acceptance or a profitability guarantee.

## Execution and Sniper state

The development package contains 16 durable execution identities. Broker ownership remains append-only: `generated=410613`, `liquidity=410614`, `trend_pullback=410615`. It includes Scalping, Swing, Expert and Sniper modes; coordinator/advisor/SOLO/RETURNING membership; durable intent/reconciliation; split-fill and uncertain-send recovery; Portfolio Exposure v1; account-scoped commissions; configurable Sniper hold; and the native Control Center.

Sniper has a native Windows MT5 `OnTick` + `CopyTicks(..., COPY_TICKS_ALL)` path and an automatic 5 ms Python polling fallback. Fast quotes use execution-only `PUT /v1/execution/pulse`; final `/v1/execution/authorize` remains mandatory. The Python adapter rechecks the account and preserves the durable `broker-journal.sqlite` commit-before-`order_send` boundary with no blind resend after uncertain outcomes.

The final Core authorization and MT5 account check run after the durable journal and immediately before `order_send`. Acceptance exposes `post_journal_authorize_ms` and `post_journal_account_ms` separately.

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

## Exact CI evidence

PR #55 tested head `892807ec317c856b1215efb569e726554266709b`, merged `development` commit `2e05cf3b907073c5faebe25fd16d4a4a523b224d`; matching source tree `a85fa0c424e23cf9e82790c649d25461e1547387`.

- Research: SUCCESS `39067458928`. Artifact `12114515035` (`synthetic-research-validation`), 4309079 bytes, SHA-256 `40d2a26180ad9d054d63a9069a7405b9e33b0ec6b89c813dad8d09ad6485263a`.
- Windows/Linux: SUCCESS `39067458927`. Portable artifact `12114566161` (`BrainTrade-Windows-x64-Preview`), 34569115 bytes, SHA-256 `79cbde840d3ff0d809da922ea55d9b3551a6007ef2301b12a76e045d31db8f70`.
- Windows validation: artifact `12114167824`, 1704761 bytes, SHA-256 `1a7a691d35acb7fa52ea941fa5db35a6064c8d013187180f1a84b47ed3dd87c1`.
- `docs/development-validation.json` contains machine-readable evidence; `docs/QUALITY_AUDIT.md` documents corrected defects and explicit limitations. Promoted 0.6.0 records are untouched.

## Next steps

1. Run the packaged `Run-Demo-Acceptance.cmd` on the intended Windows + MT5 Demo machine and collect separate Native and Poll sessions.
2. Export the privacy-safe share bundle and inspect the measured local timing components and broker result states before changing execution architecture.
3. If local P95/P99 misses the 20 ms queue-to-send target, optimize only the dominant measured component.
4. Accumulate genuine Trend Pullback/coordinator Demo and OOS evidence, including spread, fees, slippage and regime/session behavior.
5. Keep causal research lanes observational until independent evidence supports any promotion design.
6. Test private Telegram delivery with credentials stored only on the target machine.
7. Enable required branch checks when repository administration access is available.

Startup/restart remains STOPPED. SOLO never starts global execution automatically. CI does not establish profitability, guaranteed latency, broker execution quality or real native MT5 acceptance.
