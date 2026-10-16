# Continue from verified BrainTrade 0.7.0 Development Preview

Current `development` runtime/source merge checkpoint is `7b127bf03fb45044ee53dc2366a6d0c53e7c59d8` (tree `10d1d6bee091b02fa1d044336649e645bce1b177`). The exact PR #20 head `58e41284334d6778a3bfd2aa7eb5dd9cca5628fd` has that same tree and passed Research plus Windows/Linux validation before merge. Read `BUILD_HANDOFF.md`, `PROJECT_STATE.md`, `docs/development-validation.json` and `docs/ACCEPTANCE.md` before changing runtime code. Do not restart the version bump or reuse old 0.6 artifact evidence.

Implemented development work now includes Portfolio Exposure v1, four modes, SOLO/RETURNING roles, adaptive coordinator evidence, durable execution recovery, the Sniper Fast Lane and the native MT5 Sniper quote bridge. The native path uses `OnTick` to wake, `CopyTicks(..., COPY_TICKS_ALL)` to recover terminal-cached ticks, an authenticated local named pipe and distinct same-millisecond event ordering. The prior 5 ms Python quote poll remains an automatic fallback. The EA is transport-only; broker orders remain exclusively in the journaled Python execution adapter.

The Sniper engineering target is <=20 ms for BrainTrade-local queue-to-send latency, measured from the stored component timings. Do not turn that target into a broker-fill, network, profitability or HFT guarantee. Native Windows/Demo measurements are still required.

## Continue in this order

1. Finish/sync development evidence without changing promoted 0.6 records.
2. Run native Windows + Demo MT5 acceptance from `docs/ACCEPTANCE.md`, including native bridge compilation/attachment, fallback, same-millisecond ordering, partial-write recovery and P50/P95/P99 latency characterization.
3. Test private Telegram delivery only with local credentials.
4. Start aligned multi-symbol correlation/exposure **research only**: exact timestamp joins, explicit availability masks, no forward-fill and no broker execution policy.
5. Continue genuine out-of-sample/ablation/event-replay evidence.

Startup/restart stays STOPPED. Imported ML challengers remain research-only and cannot automatically promote to Champion. Correlation-driven broker execution is not implemented. CI validates software behavior with simulated MT5 boundaries; it does not prove real broker execution quality.
