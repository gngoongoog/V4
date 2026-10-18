# Continue BrainTrade 0.7.0 Development

GitHub `gngoongoog/V4` is canonical. Continue from `development`; do not restart from `main` unless preparing a promoted release.

## Student learning iteration — October 18, 2026

The user explicitly requested this extension after the Flexible Demo delivery. The current source adds a persistent entry/closed-outcome journal, context-specific advisor evidence, epsilon-greedy exploration, prospective paired exit studies with later-entry validation and rollback, and the twentieth native dashboard page (Trading → Learning progress). `docs/STUDENT_LEARNING.md` explains exact scope and limits. Execution identities remain unchanged. New source must pass exact-head Research and Windows/Linux CI before normal merge; Windows/MT5 native Demo acceptance remains a user-side task. Do not represent quote-only exit results as booked broker PnL or guaranteed improvement.

Local checks passed: 95 domain and 76 runtime Rust tests (one public-network test ignored); 128 Python passes/one OS skip; Core + simulated broker execution and the extended $100 Flexible Demo integration. Native page rendering includes page 19 at both viewport sizes. CI/package evidence is to be recorded after completion.

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
