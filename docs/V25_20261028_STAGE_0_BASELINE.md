# BRAINTRADE V25 — Stage 0 baseline, 2026-10-28

This is a code and test baseline, not a claim of Windows, MT5, trading, or
learning acceptance. No production database or user data was accessed.

## Reference and rollback

* Repository: `gngoongoog/V4`; `main` at inspection:
  `63f00265e2fac2bcf1fe94c3119dbc5ce0ae916f`.
* Previously delivered Windows source head:
  `7ef679c337edb0205d8d3c0fd18131b3a0c6ebec`.
* PR #205 is open, unmerged, and based on that head. Its branch is
  `audit/v25-native-reconciliation-hardening-20261027`, at
  `13f189b7160b401c760abf5e8eb537e1af01fb06`.
* This work starts on `fix/v25-full-exit-lifecycle-20261028` at the PR #205
  head. Both `main` and the delivered source are ancestors of that head.
  The latter is the rollback point for this assignment; `main` is unchanged.
* The selected head is 38 commits and 17 changed files ahead of `main`.
  PR #205 adds 14 commits and changes 10 files relative to the delivered head.
  Its repairs include native pipe rebind and diagnostics, a read-only broker
  reconciliation report, and avoidance of false settings-save failures after
  a durable write. These are not being recreated in stage 1.

## Delivered package provenance

The Library archive `BRAINTRADE_V4_ENTERPRISE_V25_PORTABLE_Windows_x64_7ef679c3.zip`
has SHA-256 `ba48735588a33dd6b4dc7cc1faf4dffb5776292c1d965b59ee8be211fd940858`.
Its `release-evidence/build.json` names synthetic PR merge commit
`627d011a95602977a8df159ab687b7435875efa4`, whose parents are the `main`
and `7ef679c3` commits above. GitHub reports merge tree
`80766ae31262801d2255a9db4f19cf0026cdeaba`, identical to the tree of
`7ef679c3`. This explains the different commit identifiers; it is not evidence
that those source trees differ. The archive's executable SHA-256 is
`b3adfe3ac94991574c9824ce69f71c888ba1b10ae354b57a73db73ade92b434f`, matching
the package's own build evidence. Its EA, gateway and native bridge sources
match `7ef679c3` after normalizing CRLF/LF. The EA source is version 1.11;
there is no verified compiled EX5 in this package. The PR #205 head has later
changes. Package provenance and self-reported hashes do not independently
prove runtime behavior or broker execution.

## Baseline checks

* Local Linux Python `python3 -B -m unittest discover -s tests -v` at
  `13f189b7`: **517 passed, 2 skipped**, total 519. Skips: an integration
  requiring a built local Rust core, and a real Windows named-pipe test.
* Local `npm test`: **blocked** after initial source-contract checks because
  `typescript` and the project dependencies are not installed. This is not a
  full pass. Rust/Cargo and Windows/MetaEditor/MT5 are unavailable locally.
* GitHub check runs for `13f189b7`: seven succeeded (linux-tests, windows,
  rust-source, offline-audit, research, portable-windows and native-webview),
  one `build-only` was skipped. These are checks for the existing commit,
  not for later commits, and they are not evidence of live MT5 behavior.

## Stage 1 starting findings

1. In `apps/tauri/src/bridge.rs`, the status worker may call `ensure_core()`
   before the shutdown flag is set. Other commands also open clients via the
   auto-start function. A successful `/v1/shutdown` response confirms a
   request was accepted, not that the Core and gateway processes exited.
2. In `crates/runtime/src/lib.rs`, `stop_gateway()` kills and waits for the
   recorded child without checking its results; Python cleanup in
   `gateway/mt5_gateway.py` is consequently not guaranteed to run. Safe
   shutdown already rejects active trading, unsettled commands, open/pending
   positions, and stale/unverified previously observed broker state.
3. In `apps/terminal/src/terminal.tsx`, Full Exit closes the frontend following
   the shutdown acknowledgement; it does not prove both processes are gone.
   Window-only closing intentionally leaves the Core running.
4. Actual persistent/orphan processes on the user's Windows machine are
   reported by the user; their exact cause requires Windows process evidence.

Do not delete trading or learning data, alter ambiguous broker-command states,
claim an MT5 result from a mock, merge to `main`, or mark stage 1 complete
until its real Windows process-lifecycle acceptance is demonstrated.
