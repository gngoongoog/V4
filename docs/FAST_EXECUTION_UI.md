# 0.8.0 source changes — 2026-10-16

This is a source update without a Cargo build, Windows package or CI dispatch.

- Persistent loopback HTTP with TCP_NODELAY. A lost request is never replayed by the client.
- `/v1/gateway/pulse` accepts the fresh snapshot and returns a command in one exchange. Execution runs before periodic research.
- MT5 candle, deal-history and research reads run in a bounded, separately initialized read-only process. Account/server/symbol/currency checks reject mixed results. A stalled worker can restart independently. Only the parent has the order adapter and journal.
- Sniper targets 20 ms cycles, active other modes 50 ms, idle 250 ms. Sleep subtracts processing time. The UI reports observed cycle and p95/Core latency; order send timings remain in the ledger. These are scheduling targets, not measured broker-speed promises.
- Completed-bar policies are reused for at most one second; tick-sensitive policies are evaluated per pulse. Core housekeeping is throttled and prospective member learning runs on the periodic analysis path.
- Close commands have priority. Timed exits may retry a known rejection after 250 ms; active/unknown submissions retain reconciliation barriers. Broker SL/TP continues independently.
- Fast-path intents skip the redundant broker precheck. Contract/volume/quote validation, account binding, final Core authorization, durable journal and no-replay behavior remain.
- One-click Demo binds the current fresh Demo account and disables optional per-brain risk/cooldown, aggregate portfolio and Sniper cost-share gates. It preserves chosen lots, strategy triggers, stop/target geometry and SOLO membership. It does not turn a Live account into Demo or enable broker permissions.
- New expansion brain: 8-bar compression versus the preceding 24, fresh executable breakout, independent Magic 410614, selected timeframe, SOLO/advisor support and prospective learning.
- All original 18 pages remain, grouped into five sidebar sections; the later research-desk continuation adds three pages (21 total). Start/stop stays visible; advanced execution settings and the full ledger are collapsed by default.

## Validation boundary

76 Python tests passed after the research-desk continuation using fake MT5 and a local HTTP server. The backup-journal integration test needs a built Core and was not run. All Rust files parsed without syntax errors; no Rust type-check or unit-test execution was performed. The UI loaded with a prebuilt Slint interpreter, without building the desktop application. Real MT5 startup, broker latency, order execution, packaged multiprocessing behavior and Windows rendering still require target-machine validation.

Periodic research still shares the Core request thread. The read-only MT5 process can also contend with the terminal internally. Those remaining delays must be measured on the actual terminal and broker; 20 ms is not a guaranteed round trip.
