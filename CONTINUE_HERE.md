# Continue BrainTrade 0.7.0 Development

GitHub `gngoongoog/V4` is canonical. Continue from `development`; do not restart from `main` unless preparing a promoted release.

## Current verified checkpoint — feedback repair (2026-10-19)

PR #61 is merged into `development` as `959246e97dc1be81d1b38845fa22684025e07c91`. Exact tested PR head `fb52100632d8665aa9ab8b31c989683c9982ebc6`, packaged CI merge `2c664bfff8052411a6772f25d7997636b8bededf` and the development merge share source tree `a61cb9959dbc35f8b613ef1f0e18a8e83bfe94c3`.

Research run `39181535218` and Windows/Linux run `39181535246` succeeded. The Windows job passed 97 domain, 81 runtime and 3 desktop Rust tests (one public-network test intentionally ignored), plus 129 of 130 Python tests (one OS-specific skip). Core lifecycle/config/auth, backup/restore, all four modes, simulated execution and the $100 Flexible Demo/student integration passed. Packaged executable startup, DPAPI, STOPPED default, student API and restart passed. All 40 frames of the 20-page native UI and the browser report interactions passed.

The repair completes recovered fill price/volume evidence, registers student experiences on execution acknowledgement, captures executable quotes before delayed position history and trims observations after broker close. It distinguishes quiet prices from missing receipt coverage, pools sparse regimes without double counting, weights exploration toward under-observed ready contexts and records every complete loss review once. The learning page now explains actual admission, legacy unknown admission, exclusions, repeated loss patterns and aggregated entry blockers. The 16 execution identities and Demo-only learned application are unchanged; existing studies remain stored while new known-regime studies collect independently.

Windows handoff: `BrainTrade-Windows-Feedback-fb52100.zip`, 36,143,059 bytes, SHA256 `6a430d5b4c670fa145e4f9aeb374c33be892fb42c206dc7cfb5b2ec14db5beb7`. This is the unchanged inner package from the successful Windows CI artifact. Read `docs/feedback-repair-validation.json` for provenance and `docs/STUDENT_LEARNING_AR.md` for use and limitations.

This feedback repair is complete and the Windows package is ready for target-machine use. Native Windows/MT5 Demo acceptance, actual broker latency and profitability remain unverified. Next, collect Demo acceptance evidence on the user's machine. Entry-timing experiments and exit alternatives beyond Baseline/Quick/Patient remain future work. Preserve persisted learning and previous checkpoints. The delivery documentation changes no tested execution code.

## Previous verified checkpoint — student learning

PR #60 is merged into `development` as `6467d2022e600c944b6b05a02de541e66dea78e5`. Exact tested head `126c7f357f4ed2dca0769cb8fd8ac96a843d3a22`, packaged CI merge `fef0d29e8d2517778436f0c627b3e9a7fcda4263` and the development merge share source tree `ab415df5284b60ea7e5eb0ffb5d7435d349b1e4c`.

Research run `39152599211` and Windows/Linux run `39152599258` succeeded. Windows passed 95 domain, 77 runtime and 3 desktop Rust tests (one public-network test ignored), plus 128 Python tests (one OS-specific skip). Portable startup, restore, the student API, all 40 rendered frames of the 20-page UI, execution integration and the $100 Flexible Demo/student integration passed.

This adds a durable entry/closed-outcome journal, context-specific advisor evidence, reproducible epsilon-greedy exploration, prospective paired exit studies with later-entry validation and rollback, and Trading → Learning progress. The learner identity stays stable when monetary tick values change; every experience retains its frozen calculation values. Learned Sniper targets retain the configured cost fraction, with an explained per-entry baseline fallback.

Read `docs/STUDENT_LEARNING_AR.md` for the user report and `docs/STUDENT_LEARNING.md` for evidence rules. `docs/student-learning-validation.json` records exact source, CI and delivery hashes. Execution identities remain the same 16; quote-only shadow comparisons must never be described as broker PnL. Automatic exit application is Demo-only and affects future entries.

The preceding student-learning build is complete and its Windows package was delivered. Native Windows/MT5 Demo acceptance and profitability remain unverified; collect target-machine results next. Do not reset persisted learning or overwrite prior checkpoints. This evidence-only documentation commit does not change tested execution code.

## Previous verified checkpoint — flexible Demo learning

PR #59 is merged into `development` as `317df185f035e2f3d90387fcf6e3e13babd05327`. Exact tested PR head `710b31ffa906920e4ffda2930a18b020d45e4e5f`, packaged CI merge `b5a22971422b1431f8a9ada6e7f60d169b8241f7` and the development merge share source tree `f0e5b5c94490decfa353257892748af0ed394b8b`.

Research run `39084459002` and Windows/Linux run `39084459007` both succeeded. Windows passed 92 domain, 73 runtime and 3 desktop Rust tests (one public-network test intentionally ignored); 128 of 129 Python tests passed (one OS-specific skip). The packaged executable passed the existing execution integration and the new $100 flexible Demo integration.

This checkpoint adds Demo-only actionable-advisor selection with learned weight plus bounded exploration, downsize-only margin-aware lots, exact closed SOLO/primary evidence, selected-advisor credit, complete-volume closure checks, incremental learning reconciliation and native cost/exit/weight explanations. The Trading Center switch defaults on for Demo, is stopped-only to change, and does not apply to Live accounts or implicitly START execution. There are still 16 append-only execution identities.

Read `docs/FLEXIBLE_DEMO_AR.md` for behavior and use. `docs/flexible-demo-validation.json` records this checkpoint and deliverable hashes. Real Windows/MT5 Demo acceptance, broker latency and profitability remain unverified; the next task is target-machine acceptance, not speculative new speed changes.

## Previous verified checkpoint

PR #55 (comprehensive 0.7 correctness audit) is merged into `development` as `2e05cf3b907073c5faebe25fd16d4a4a523b224d`. Exact tested PR head `892807ec317c856b1215efb569e726554266709b` and the merge share code tree `a85fa0c424e23cf9e82790c649d25461e1547387`. Research run `39067458928` and Windows/Linux run `39067458927` both succeeded on that code.

Verified count: 87 domain Rust tests, 72 runtime Rust tests plus one intentionally ignored official-news public-network test, three desktop Rust tests, and 124 of 125 Windows Python tests passed (one OS-specific fixture skipped). The Windows CI executed the real named-pipe blocking-connect shutdown/rebind regression, built the portable preview and passed simulated execution integration. The Research CI passed browser report and complete native-page rendering. Real Windows/MT5 Demo acceptance remains pending.

PR #55 fixed native pipe shutdown/rebind and dead-worker detection, stopped-only execution-affecting Risk/Trading settings, side-effect-free rejection of invalid `/v1/config`, explicit-only START, final post-journal Core authorization and account recheck before `order_send`, safe timeframe indexing, accurate START audit metadata and complete Python compile checks. It adds `post_journal_authorize_ms` and `post_journal_account_ms` latency evidence without double counting.

Promoted 0.6.0 evidence remains immutable. BrainTrade 0.7.0 is a Development Preview, not production/broker acceptance or a profitability guarantee.

The development line has 16 durable execution identities. Broker magic ownership remains append-only: `generated=410613`, `liquidity=410614`, `trend_pullback=410615`. `docs/development-validation.json` holds the exact machine-readable evidence; promoted 0.6.0 evidence remains separate.

## Implemented engineering

- Coordinator/advisor/SOLO/RETURNING execution membership.
- Scalping, Swing, Expert and Sniper modes.
- Sniper Fast Lane, execution-only Fast Pulse and native MT5 `OnTick`/`CopyTicks` bridge with automatic polling fallback.
- Durable pre-send journal, uncertain-send reconciliation and no blind resend; second Core authorization and final MT5 identity recheck after journal and immediately before `order_send`.
- Exact account binding, quote freshness, ownership and final authorization boundaries.
- Account-scoped commissions, configurable Sniper hold and Portfolio Exposure v1.
- Native five-section Control Center and persisted runtime state.
- Correlation/OOS/feature-ablation/news-replay research lanes that remain observational.
- Exact closed Demo broker-outcome adaptive learning and Intelligence V3 weighting.
- Trend Pullback using completed bars only with EMA20/EMA50 structure, controlled pullback, confirmation, anti-chasing and structural stop geometry.
- Packaged read-only Demo acceptance workflow with `Run-Demo-Acceptance.cmd`, a Windows GUI Acceptance Center, the advanced PowerShell CLI and Arabic instructions.
- GUI cards for result/sample counts/queue-to-send P95/P99/send P95/Native-vs-Poll timing plus controlled Native/Poll/Auto capture sessions.
- Privacy-safe acceptance evidence export using a strict allowlist; raw login, IPC key, broker databases/journals, Telegram credentials and GUI logs are excluded.
- Windows parser validation of the acceptance PowerShell source and packaged copies before delivery.

## Continue in this order

1. **Run native Windows + real Demo MT5 acceptance on the target machine.** Open the packaged `Run-Demo-Acceptance.cmd`; collect a `native` session with the bridge attached and a separate `poll` session with it detached/stopped.
2. **Export the share bundle.** Use `تصدير حزمة مشاركة` after the captures so the acceptance evidence can be reviewed without account secrets or broker databases.
3. **Measure before further speed changes.** Use `queue_to_send_ms`, `core_pulse_ms`, `authorize_ms`, `journal_ms`, `post_journal_authorize_ms`, `post_journal_account_ms`, `other_pre_send_ms`, `pre_send_ms`, `send_ms` and `total_ms` to identify the actual bottleneck.
4. **Evaluate Trend Pullback with real Demo evidence.** Track closed outcomes, fees, spread/slippage, regime/session behavior and enough prospective/OOS samples before changing learned influence.
5. **Keep broker correctness boundaries intact.** Do not remove exact account identity, quote freshness, ownership, final authorization, durable journal or uncertain-send reconciliation merely to improve a benchmark.
6. **Keep imported ML challengers research-only** until independent evidence and an explicit promotion design exist.
7. **Test private Telegram delivery** only with target-machine credentials.
8. **Enable required branch checks** for `main` and preferably `development` if repository administration access is available.

## Important boundaries

- BrainTrade 0.7.0 is a Development Preview, not the promoted 0.6.0 release.
- Startup/restart stays STOPPED.
- The <=20 ms Sniper goal is BrainTrade-local queue-to-`order_send` handoff, not broker acknowledgement or fill latency.
- CI uses simulated broker boundaries and does not prove native MT5 acceptance, real fill quality, profitability or guaranteed latency.
- The Demo acceptance collector/GUI is read-only and must not manufacture trades.
- The privacy-safe share exporter is diagnostic only and must stay allowlist-based; do not broaden it to runtime databases, secrets or raw account login data.
- Research correlation, ablation, news replay and imported ML do not automatically authorize broker execution.

When changing execution code, use a focused branch, preserve append-only broker magic ownership, run exact-head Research + Windows/Linux CI, then merge normally into `development`.
