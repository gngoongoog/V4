# Current development — Enterprise V25 RC4 (0.25.3), 2026-10-27

Work on `feature/v25-rc4-strategy-learning-cycle-20261027`, based on RC3 `a42731abc3ecdb664675bae07bf6efc6eb5d3e2d` (PR #202). Main remains tested RC2 `63f00265e2fac2bcf1fe94c3119dbc5ce0ae916f`; do not merge automatically.

Current scope: bounded training-only strategy evolution, persisted causal parent lineage, complete trial diagnostics, modern Studio controls, explicit exact-report Demo Expert advisor approval, and broker-feedback regression coverage. No automatic REAL research promotion. See `docs/v25/RC4_STRATEGY_LEARNING_AR.md`. Source is pending CI until evidence is recorded; native MT5, profitability and independent OOS remain unverified.

Earlier sections below are historical.

# Current review — Enterprise V25 RC3 (0.25.2), 2026-10-27

Main baseline: `63f00265e2fac2bcf1fe94c3119dbc5ce0ae916f` (RC2), promoted by explicit user instruction. Post-promotion Windows/Linux run `40436758683` passed. Previous main is preserved at `archive/main-before-v25-rc2-20261027`.

Continue on `feature/v25-rc3-integrity-audit-20261027`. Do not merge this review into main until its checks and review are complete. RC3 fixes the Core/dashboard evidence contract and breakeven counts; it does not change trading permissions. See `docs/v25/RC3_INTEGRITY_AUDIT_AR.md` and the branch PR for actual test results. Native MT5 Demo, EA compilation, measured broker latency, independent OOS and private Telegram delivery remain unverified. No REAL trades or model promotion.

All earlier continuation instructions below are historical.

Current development: **Enterprise V25 RC2 (0.25.1)** on `feature/v25-rc2-build-identity-g7-ui-20261026`. See `CONTINUE_HERE.md`; older entries below are historical.

# Current continuation — 2026-10-22, Stage 3F-U (v0.7.4)

Continue from latest `main`, preserving all newer changes. PR #109 (3F-T closed Demo Sniper policy quality) merged as `392a8143ae9cc2b58f52784db9f66f2cc39bfe9b` after exact-head Research `39779107057` and Windows/Linux `39779107116` succeeded on `ae7eb22ccb91edb7fd73f765e8a191fe4b5f6896`.

3F-U adds bounded, account-scoped Demo Sniper decision sampling with first-blocker reasons, exact same-evaluation order links and a separate five-second prospective quote comparison. It never changes execution, learning weights or promotion; Fast Pulse does not record these observations. Read `docs/SNIPER_OPPORTUNITY_REVIEW_3FU_AR.md`. Require successful exact-source Research and Windows/Linux checks before merging new source. Next: collect target-machine Demo broker acceptance and review measured rejection/coverage/outcome evidence before any policy change. Native MT5 fills, profitability, latency and private Telegram delivery remain unverified.

Older checkpoint instructions below are historical. Preserve learning data, immutable research evidence, strict selected-mode ownership and all broker correctness boundaries.

# Latest work: 0.7.4 P0 phase 2 risk-based maximum lot

Fresh broker-entry commands use equity and broker order_calc_profit at rounded SL to limit (never enlarge) the requested lot after including configured roundtrip commission and two-sided slippage. Above-budget broker-minimum sizes are rejected; REAL commands missing the risk budget are denied. Maintain a coherent 0.7.4 UI/Core/Gateway and verify exact SHA with Windows and Research CI. MT5 native acceptance is not established.

# Current 0.7.3 — P0 fee/lot-contract stage 1

Commission fee model for round-trip lot sizes is aligned across Core, Rust trading and risk, demo margin downsize, native preview and test suite. 7 per 1 lot -> 0.07 / 0.70 / 7.00 for 0.01 / 0.10 / 1.00; 0 means zero modeled broker commission only. Risk-sized lot based on stop is a future separately tested stage. Exact commit Research+Windows CI must pass before merging. See docs/P0_COMMISSION_STAGE_1_AR.md.

# Current 0.7.2 Demo/REAL safety audit

0.7.2 changes the Core/native protocol for REAL binding and start; it must never reuse 0.7.1 Core. New LIVE starts require fresh broker identity, exact typed login on both bind and START, permitted Algo trading and enabled portfolio protection. Research and Windows CI for the exact 0.7.2 commit are required before release. Real broker testing remains pending. See `docs/REAL_ACCOUNT_SWITCH_ACCEPTANCE_AR.md`.

# Current 0.7.1 follow-up

PR #74 adds the brain control room, scoped participation/paper controls, four Sniper learner styles and Arabic Telegram. Continue from the latest main; older checkpoints below are historical. See `docs/BRAIN_CONTROL_SNIPER_AR.md` and exact-head CI. Native MT5 demo acceptance remains outstanding.

# BrainTrade OS — verified 0.7.0 Development Preview

GitHub `gngoongoog/V4` is canonical. Continue from `development`; use `main` only when preparing a promoted release.


## Latest verified checkpoint — contextual mistake memory (2026-10-19)

PR #64 is merged into `development` as `88a341fc8733deaa1856079fc073bb744a61afcf`, source tree `59efba92fc67eb249336695074e9c67eafb7537b`. This includes PR #62 timing/five-way exits and PR #63 handoff documentation. Post-merge Research `39249735055` and Windows/Linux `39249735059` both passed on that exact merge.

Windows evidence: 103 domain, 85 runtime and 3 desktop Rust tests passed, with one public-network test ignored; 129 of 130 Python tests passed, with one OS-specific skip. Packaged startup/DPAPI/STOPPED default, all four modes, backup/restore/restart, execution integration and the $100 Flexible Demo/student integration passed. All 40 native frames cover 20 pages. Native MT5 acceptance, actual broker latency and profitability remain unverified.

The current learner reduces a ready Demo advisor's preference after at least three recent consecutive booked losses in the same strategy/market context. The penalty is capped at 0.20, clears with a newer non-loss or expiration of the 14-day window, and never vetoes a valid candidate. Exploration graduates from 30% toward 5% using total eligible contextual evidence while retaining novelty-weighted sampling. The entry journal shows the frozen streak and penalty. Five-way exits, legacy studies, research-only timing and 16 execution identities remain intact.

Windows handoff: `BrainTrade-Windows-Learning-88a341f.zip` (36,203,052 bytes), SHA256 `0f0ffb68f16f6bd04d41e740b1735cc2b477931542b8aae76190f9f9dba8c6bb`. It is the unchanged inner package from successful post-merge artifact `12186286037`. Read `docs/LATEST_WINDOWS_HANDOFF_AR.md` and `docs/latest-learning-validation.json`. Take an application backup before upgrading; older binaries cannot read new five-way study records. Native Demo acceptance is the next evidence step; automatic application of timing recommendations remains unimplemented. This delivery documentation changes no tested execution code.


## Previous delivered development checkpoint — PR #62 (2026-10-19)

Continue from `development` merge `29fe6663972651e8e3a1b613b9d957a86590bf6e` (PR #62). **Post-merge Research PASS**: [39240501418](https://github.com/gngoongoog/V4/actions/runs/39240501418); **post-merge Windows/Linux PASS**: [39240501431](https://github.com/gngoongoog/V4/actions/runs/39240501431). Windows preview upload: `BrainTrade-Windows-x64-Preview`, artifact `12180999893`, 34,895,029 bytes, Actions artifact ZIP SHA-256 `7f48e39954bfa7fee0be36939b747e00e3998032745dd0048096f6b74ca119b6`.

Features: prospective five-way exits with preserved three-way history, independent quote-only 0/250/1000 ms entry-timing research and explicit coverage/exclusion evidence. Timing is not a broker execution policy; booked MT5 PnL is not inferred from comparative quotes. There are still 16 durable broker identities. Read `docs/ENTRY_TIMING_EXIT5_AR.md` and `docs/pr62-learning-validation.json`.

**Important remaining gate:** target-machine Windows + native MT5 **Demo** acceptance has not been performed. Use `Run-Demo-Acceptance.cmd` from the downloaded package to collect separate Native/Poll samples and share its privacy-safe evidence bundle. Validate actual fills/costs, timing and continued learning before promoting any execution decision. PR #58 remains open and requires compatibility review. Previous PR #55 evidence below is a historical correctness checkpoint, not the latest tested source.

## Earlier verified correctness checkpoint — PR #55

PR #55 (comprehensive 0.7 correctness audit) is merged into `development` as `2e05cf3b907073c5faebe25fd16d4a4a523b224d`. Exact tested PR head `892807ec317c856b1215efb569e726554266709b` and the merge share code tree `a85fa0c424e23cf9e82790c649d25461e1547387`. Research run `39067458928` and Windows/Linux run `39067458927` both succeeded on that code.

Verified count: 87 domain Rust tests, 72 runtime Rust tests plus one intentionally ignored official-news public-network test, three desktop Rust tests, and 124 of 125 Windows Python tests passed (one OS-specific fixture skipped). The Windows CI executed the real named-pipe blocking-connect shutdown/rebind regression, built the portable preview and passed simulated execution integration. The Research CI passed browser report and complete native-page rendering. Real Windows/MT5 Demo acceptance remains pending.

PR #55 fixed native pipe shutdown/rebind and dead-worker detection, stopped-only execution-affecting Risk/Trading settings, side-effect-free rejection of invalid `/v1/config`, explicit-only START, final post-journal Core authorization and account recheck before `order_send`, safe timeframe indexing, accurate START audit metadata and complete Python compile checks. It adds `post_journal_authorize_ms` and `post_journal_account_ms` latency evidence without double counting.

Promoted 0.6.0 evidence remains immutable. BrainTrade 0.7.0 is a Development Preview, not production/broker acceptance or a profitability guarantee.

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

The engineering target remains **<=20 ms for BrainTrade-local `queue_to_send_ms`** on the intended Windows machine. Use `core_pulse_ms`, `queue_to_gateway_ms`, `authorize_ms`, `journal_ms`, `post_journal_authorize_ms`, `post_journal_account_ms`, `other_pre_send_ms`, `pre_send_ms`, `queue_to_send_ms`, `send_ms` and `total_ms` to locate the measured bottleneck.

Do not remove account identity, quote freshness, ownership, authorization, durable journal or uncertain-send reconciliation merely to improve a benchmark. `send_ms` is MT5/network/broker dependent and is not part of the local <=20 ms claim.

## Exact CI evidence

PR #55 tested head `892807ec317c856b1215efb569e726554266709b`, merged `development` commit `2e05cf3b907073c5faebe25fd16d4a4a523b224d`; matching source tree `a85fa0c424e23cf9e82790c649d25461e1547387`.

- Research: SUCCESS `39067458928`. Artifact `12114515035` (`synthetic-research-validation`), 4309079 bytes, SHA-256 `40d2a26180ad9d054d63a9069a7405b9e33b0ec6b89c813dad8d09ad6485263a`.
- Windows/Linux: SUCCESS `39067458927`. Portable artifact `12114566161` (`BrainTrade-Windows-x64-Preview`), 34569115 bytes, SHA-256 `79cbde840d3ff0d809da922ea55d9b3551a6007ef2301b12a76e045d31db8f70`.
- Windows validation: artifact `12114167824`, 1704761 bytes, SHA-256 `1a7a691d35acb7fa52ea941fa5db35a6064c8d013187180f1a84b47ed3dd87c1`.
- `docs/development-validation.json` contains machine-readable evidence; `docs/QUALITY_AUDIT.md` documents corrected defects and explicit limitations. Promoted 0.6.0 records are untouched.

## Target-machine work still pending

1. On the intended Windows PC, open the packaged `BrainTrade.exe`, connect the intended MT5 **Demo** account and open `Run-Demo-Acceptance.cmd`.
2. Collect a controlled Native session with `BrainTradeSniperBridge` attached and a separate Poll session with it detached/stopped.
3. Use `تصدير حزمة مشاركة` after capture so the JSON/CSV/state and reduced summary can be inspected without sending account secrets or broker databases.
4. Evaluate real retcodes, fees, rejections, partial fills and P50/P95/P99 timing distributions. If local latency misses the target, optimize only the measured dominant component.
5. Accumulate genuine prospective/OOS evidence for Trend Pullback and the coordinator before changing influence or thresholds.
6. Test private Telegram delivery with credentials stored only on the target machine.
7. Enable required branch checks when repository administration access is available.

Startup/restart remains STOPPED. CI does not establish profitability, broker fill quality, guaranteed latency or native MT5 acceptance. The Demo acceptance collector and Acceptance Center remain read-only and must not manufacture trades.
