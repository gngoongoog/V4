# BrainTrade OS — 0.7 Development Quality Audit

Audit date: 2026-10-17. Canonical repository: `gngoongoog/V4`.

This audit covers the 0.7.0 Development Preview execution path, MT5 gateway, native Sniper bridge, Core authorization, persistence/recovery, trading/risk configuration, native desktop, research/learning boundaries, Windows packaging, acceptance tooling and CI. It is a correctness/stability audit; it is **not** a profitability claim, live-account approval, or fixed broker/network/fill-latency guarantee.

## Audited executable checkpoint

- Pull request: `#55` — Comprehensive 0.7 correctness audit fixes.
- Code head: `b227181083b4b72f2de70e4f9c5f0f55a77e9b9f`.
- Code tree: `323bd8140ba65608493243ba3df4c1c19ec735a8`.
- Base: `development` at `8dc0c5c2f967e326ff959fc168db303d86f5048c`.
- The PR deliberately avoids speculative latency tuning until target-machine Windows + MT5 Demo evidence exists.

## Confirmed defects fixed in this audit

### Native Sniper bridge lifecycle

- Windows named-pipe shutdown now cancels blocking synchronous I/O and joins the worker with a bounded deadline.
- A stuck old pipe server cannot have its token/data-path state silently rotated underneath it.
- A dead worker is no longer reported as available; the next terminal bind recreates the server.
- A Windows-only regression test blocks in `ConnectNamedPipe`, stops the server, then binds a second server to the same pipe name. Windows CI runs the full Python suite, so this test is active on the Windows runner rather than skipped by its non-Windows guard.

### Execution/configuration races

- Risk and trading-cost changes are rejected while global START is armed or any broker command remains unresolved.
- Invalid `/v1/config` requests are now fully parsed/validated before queued entries are cancelled or START is disarmed. Rejected settings are side-effect free with respect to run state.
- Execution brain configuration/role changes can no longer arm global START through a wrapper fallback. Global START is explicit only through `/v1/start`.
- START audit metadata now states the real execution boundary instead of recording the old `execution:false` wording.
- The execution timeframe selector is bounds-safe instead of indexing the frame table directly.

### Final pre-send race

The broker adapter now closes the material post-journal/pre-send window without weakening the durable no-duplicate contract:

1. validate broker/account/quote/ownership/geometry;
2. perform the first Core authorization;
3. recheck account/mode/expiry;
4. commit the send intent with SQLite `PRAGMA synchronous=FULL`;
5. perform a **second Core authorization after the durable journal**;
6. perform a **final MT5 login/server/currency/trade-mode/expiry check**;
7. call `order_send` only if both final gates still pass.

If STOP, execution revision, brain enablement or account identity changes after the journal commit, the adapter persists `cancelled` and does not call `order_send`. Duplicate delivery cannot resurrect that cancelled intent. Regression tests cover both STOP-after-first-authorization and account-switch-after-journal cases.

This is not described as cross-process atomicity: Core and MT5 remain separate processes. The dangerous material window is reduced by placing explicit final checks immediately before the broker send.

## Execution invariants re-reviewed

- Account identity and Demo/Live target must match the bound context.
- Quote freshness, symbol and broker contract geometry remain mandatory.
- Stable brain ownership/magic identities are preserved.
- Queued commands are invalidated when relevant execution configuration changes.
- Dispatched opens must match the current execution revision at authorization time.
- The second post-journal authorization re-reads command state from SQLite and rechecks context, expiry, revision, execution-enabled state and brain-enabled state for opens.
- Owned closes intentionally remain authorizable while global START is off so existing positions can retain their exit management.
- `queued`, `dispatched`, `unknown`, `placed`, `filled` and `partial` all remain unresolved barriers where configuration mutation or duplicate ownership would be unsafe.
- Partial fills are never automatically topped up and retain an ownership barrier until broker position/deal reconciliation completes.
- Unknown/interrupted sends are reconciled from broker state/history and are never blindly resent.
- The durable journal is committed before every broker send. `synchronous=FULL` is intentionally preserved.
- Netting accounts refuse independent ownership merges; hedging ownership remains magic/ticket scoped.
- STOP prevents new entries while existing owned exits retain their intended management semantics.

## Costs, learning and research boundaries

- Trading costs remain account-scoped. Configured commission is proportional to lot size and enters trading/risk logic.
- Exact Demo learning uses broker-booked `profit + commission + swap + fee` when MT5 associates those deals with a position.
- Fee/cashflow deals without a usable position link are not guessed onto an individual brain.
- Imported ML challenger inference remains research/audit-only. No automatic champion promotion or broker authority exists.
- News/replay, correlation and historical strategy research remain evidence layers rather than hidden execution authority.
- Historical/synthetic metrics are implementation evidence, not evidence of future win rate or profitability.

## Persistence, secrets and local boundary

- Core HTTP remains bound to `127.0.0.1`, requires the local bearer IPC key, rejects browser `Origin` requests and enforces request-size limits.
- SQLite uses WAL, `synchronous=FULL`, foreign-key enforcement and startup integrity checking.
- Backups exclude local secrets and do not restore the gateway send journal, preventing historical broker intents from being rewound into a new send state.
- Telegram remains a private/read-only information path and has no order-execution authority.
- A valid config request that later encounters an I/O/DPAPI persistence failure remains fail-safe rather than continuing execution with silently half-applied runtime state.

## Validation — PASS at the audited code checkpoint

### Research source validation

Run `39066217891`: **SUCCESS** on exact head `b227181083b4b72f2de70e4f9c5f0f55a77e9b9f`.

Passed coverage includes Rust domain/runtime tests, Core build, compile-check of all Python under `gateway`, `scripts` and `tests`, Python unit tests, real-Core smoke, backup/restore, Strategy Studio, execution integration, offline browser report interaction, native selector compilation, all native desktop page renders and artifact upload.

Research artifact:

- name: `synthetic-research-validation`
- artifact id: `12113507845`
- size: `4,308,946` bytes
- SHA-256: `23e060ab1ada9f18c4c1b738722eee458fb5046f798847218a6064436d03db3`

### Windows build and tests

Run `39066217909`: **SUCCESS** on the same exact head.

The Linux duplicate-validation job also completed successfully. The Windows packaging step runs workspace Rust tests, the full Python test discovery on Windows, release desktop build, embedded-runtime check, portable package smoke and execution integration. The Windows-only named-pipe lifecycle test is guarded only against non-Windows hosts, so the successful Windows full-suite run exercises the real `ConnectNamedPipe` stop/rebind path.

Windows Preview artifact:

- name: `BrainTrade-Windows-x64-Preview`
- artifact id: `12114110976`
- size: `34,566,867` bytes
- SHA-256: `f3f334b3e12268b97208cc4b47452067636a4a48f037526f4be325aed23f45b8`

Windows native-validation artifact:

- name: `windows-native-validation`
- artifact id: `12113926713`
- size: `1,704,761` bytes
- SHA-256: `4a2d06ae208ec55f371d7691a60b9c182da2a72ef08c430fcfc455ca0ad61ea3`

The validation artifact records `PASS` for the 0.7.0 packaged Core/runtime/execution integration. It contains 19 native pages rendered at two sizes — **38 complete native frames** — plus `execution-integration.json` and `windows-portable-smoke.json`.

Current audited test totals include 87 domain Rust tests; 72 runtime Rust tests passed with one explicit public-network news test ignored; the Linux Python suite ran 125 tests with only the Windows-specific named-pipe test skipped there. Windows runs the full Python suite on the native host.

## Acceptance/latency instrumentation strengthened

The Demo acceptance collector now exposes these pre-send components separately:

- `core_pulse_ms`
- `queue_to_gateway_ms`
- `authorize_ms`
- `journal_ms`
- `post_journal_authorize_ms`
- `post_journal_account_ms`
- `other_pre_send_ms`
- `pre_send_ms`
- `queue_to_send_ms`
- `send_ms`
- `total_ms`

`other_pre_send_ms` excludes the named authorization/journal/final-account components so the report does not double-count them. Older acceptance samples that predate the two new post-journal timing fields remain readable.

The engineering target remains `queue_to_send_ms <= 20 ms` for BrainTrade-local queue-to-send latency on the target machine. The two final safety gates are intentionally included in that measurement; they must be measured before considering any latency redesign.

## External acceptance still pending — not represented as CI PASS

The following require the user's actual Windows/MT5/broker environment and remain pending:

- real MT5 Demo order acceptance, broker rejections, partial fills and network behavior;
- Native OnTick vs 5 ms fallback-polling P50/P95/P99 on the target machine;
- real cost of `post_journal_authorize_ms` and `post_journal_account_ms` under the user's MT5 installation;
- actual broker spread/commission/slippage/session/regime evidence for prospective strategies such as Trend Pullback;
- target-machine Native EA compile/attach/pipe acceptance against the selected broker symbol;
- live private Telegram delivery;
- enough fresh real out-of-sample evidence to make any effectiveness claim about research challengers.

CI cannot substitute for these checks. No fixed broker acknowledgement/fill latency is claimed.

## Repository/admin limitation

`development` is currently not protected by required status checks. This is a repository administration setting outside the write permissions available to this audit workflow; it is recorded as an external governance limitation rather than falsely marked fixed.

## Non-blocking observations

- A small set of compiler warnings concerns unused imports/test-only or legacy helper code. They are not execution defects and were not used as justification for a broad late-stage refactor.
- The Native tick mailbox is bounded and internally counts dropped packets on overflow. That counter is not yet promoted into the public telemetry schema. Expose it only if target-machine burst evidence shows it is needed for diagnosis; do not widen the execution schema speculatively.

## Next quality gate

Run the privacy-safe Demo Acceptance Center on the actual Windows + MT5 Demo machine in controlled `Native` and `Poll` sessions. Compare P50/P95/P99 and the component timings above. Optimize only the measured dominant local bottleneck while preserving account identity, final authorization, durable journaling and uncertain-send reconciliation.

Historical 0.4/0.5 audit statements are superseded by this document for the current 0.7 Development Preview. Promoted 0.6 release evidence remains immutable in its dedicated release/validation records.