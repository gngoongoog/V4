# BrainTrade OS — project state: verified 0.7.0 Development Preview

Canonical repository: `gngoongoog/V4`. Current `development` runtime/source merge checkpoint: `7b127bf03fb45044ee53dc2366a6d0c53e7c59d8` (tree `10d1d6bee091b02fa1d044336649e645bce1b177`). The exact PR #20 head `58e41284334d6778a3bfd2aa7eb5dd9cca5628fd` has the same tree and passed Research and Windows/Linux validation before merge. The post-merge Research run also passed on the merge commit; post-merge Windows packaging is tracked separately before this evidence-sync branch is merged.

The current checkpoint passed 61 domain Rust tests, 47 runtime Rust tests and 78 Python tests; one explicit public-network news test is intentionally ignored. Core, backup/restore, Studio, offline reports, simulated MT5 execution integration, browser review and native desktop rendering passed. Native MT5 Demo acceptance on the user's actual terminal is still a separate target-machine gate.

## Execution and Sniper state

The package includes 14 execution identities; Scalping, Swing, Expert and Sniper; coordinator/advisor/SOLO/RETURNING membership; proportional commissions; Wyckoff; 21-timeframe strategy generation; durable execution reconciliation; Portfolio Exposure v1; and the native dashboard.

Sniper now has two quote-arrival paths:

- a native Windows MT5 bridge (`BrainTradeSniperBridge.mq5`) that uses `OnTick` as the wake signal and `CopyTicks(..., COPY_TICKS_ALL)` to recover terminal-cached ticks that can be coalesced while an event handler is already running;
- the existing 5 ms Python `symbol_info_tick` loop as an automatic fallback when the native bridge is absent or stale.

The native bridge forwards executable Bid/Ask changes in source order through an authenticated local named pipe. Same-millisecond quote changes receive distinct local event ordering so Core does not collapse them into one Sniper decision. Partial pipe writes do not advance the MT5 tick cursor; the next event can recover the batch from the terminal tick cache. The EA contains no broker trading API. All broker submissions remain in the Python execution adapter behind account/ownership checks and the durable `broker-journal.sqlite` no-blind-retry boundary.

The fast execution path records `queue_to_gateway_ms`, `authorize_ms`, `journal_ms`, `pre_send_ms`, `queue_to_send_ms`, `send_ms` and `total_ms`. **20 ms or less is an engineering target for BrainTrade-local queue-to-send latency on the target machine, not a guaranteed broker acknowledgement, fill time, network latency or HFT claim.** Native-path P50/P95/P99 still require real Demo measurement.

Portfolio Exposure v1 remains OFF by default. When enabled while STOPPED it caps aggregate BrainTrade entry slots, lots and estimated stop risk; unknown aggregate stop risk fails closed for new entries. It never blocks owned exits, broker SL/TP, timed exit management or reconciliation.

## Evidence and delivery

`docs/development-validation.json` is the machine-readable 0.7 development checkpoint. The promoted 0.6 history in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` remains unchanged. Documentation-only evidence commits must never be described as newly compiled runtime code.

## Next steps

1. Complete native Windows + Demo MT5 acceptance from `docs/ACCEPTANCE.md`, including native/fallback Sniper latency characterization and same-millisecond/pipe-recovery checks.
2. Test private Telegram delivery using credentials stored only on the target machine.
3. Begin **aligned multi-symbol correlation/exposure research** as a research-only module: exact timestamp alignment, explicit availability masks, no forward-fill across unavailable intervals and no correlation-driven broker policy.
4. Continue genuine out-of-sample accumulation, ablation and first-observed historical-event replay.
5. Enable required branch checks when repository administration access is available.

Startup/restart is STOPPED. SOLO never starts global execution automatically. STOP blocks new entries while owned timed exits continue with Core active; Full Exit stops client-side timed exits while broker-attached SL/TP remain at the broker. Imported ML challengers remain research-only with no automatic Champion promotion. CI does not establish profitability, guaranteed latency or broker execution quality.
