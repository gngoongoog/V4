# BrainTrade OS — project state: verified 0.7.0 Development Preview

Canonical repository: `gngoongoog/V4`.

Current runtime/source merge checkpoint: `cc0ba8e84d862af706dbadec66c146c1fe47e2f2` (tree `488aa76c57c5b03719eeced3c2894ee2d065627f`). It contains the completed native Sniper work, multi-symbol correlation research A–D, persisted/replayable holdout-only OOS evidence, and deterministic Feature Ablation v1.

The exact PR #28 source head `956dacd74fa8294a3cda8d759f161a3ab41ccb40` passed Research run `38959684403` and Windows/Linux run `38959684555` before merge. On that exact source, 78 domain Rust tests passed, 59 runtime Rust tests passed with one explicit public-network news test intentionally ignored, and 82 Python tests passed. Core smoke, backup/restore, Studio, simulated MT5 execution, browser review, native desktop compile/render and Windows portable packaging passed. Post-merge validation of `cc0ba8e...` is recorded in `docs/development-validation.json` once both push workflows complete.

Native Windows + Demo MT5 acceptance on the user's actual terminal is still a separate target-machine gate and is not implied by CI.

## Execution and Sniper state

The package includes 14 execution identities; Scalping, Swing, Expert and Sniper; coordinator/advisor/SOLO/RETURNING membership; proportional commissions; Wyckoff; 21-timeframe strategy generation; durable execution reconciliation; Portfolio Exposure v1; and the native 19-page dashboard.

Sniper has two quote-arrival paths:

- native Windows MT5 bridge `BrainTradeSniperBridge.mq5`, using `OnTick` as the wake signal and `CopyTicks(..., COPY_TICKS_ALL)` to recover terminal-cached ticks that can be coalesced while an event handler is already running;
- the 5 ms Python `symbol_info_tick` loop as automatic fallback when the native bridge is absent or stale.

The native bridge forwards executable Bid/Ask changes in source order through an authenticated local named pipe. Same-millisecond quote changes receive distinct local event ordering. Partial pipe writes do not advance the terminal tick cursor, allowing recovery from the MT5 tick cache on the next event. The EA contains no broker trading API. All broker submissions remain in the Python execution adapter behind account/ownership checks and durable `broker-journal.sqlite` no-blind-retry persistence.

Fast execution records `queue_to_gateway_ms`, `authorize_ms`, `journal_ms`, `pre_send_ms`, `queue_to_send_ms`, `send_ms` and `total_ms`. **20 ms or less is an engineering target for BrainTrade-local queue-to-send latency on an appropriate target machine, not a guaranteed broker acknowledgement, fill time, network latency, capture of every upstream event, profitability or HFT claim.** Native/fallback P50/P95/P99 still require real Demo measurement.

Portfolio Exposure v1 remains OFF by default. When enabled it caps aggregate BrainTrade entry slots, lots and estimated stop risk. Unknown aggregate stop risk fails closed for new entries. It never blocks owned exits, broker SL/TP, timed exit management or reconciliation.

## Research state

### Multi-symbol correlation A–D — complete as research

- exact M1 alignment from immutable saved backtest histories;
- explicit per-symbol availability masks and no forward-fill through missing intervals;
- pairwise return/correlation statistics on valid intersections only with explicit minimum overlap;
- all-symbol common intersection for portfolio covariance/volatility;
- deterministic source/config identity;
- persistence and research-only API;
- native correlation UI in the 19-page dashboard;
- deterministic replay/export, source-mutation detection, database-restart persistence and backup/restore coverage.

Correlation results do **not** affect Portfolio Exposure v1, coordinator votes or broker execution.

### OOS evidence — complete foundation

Holdout-only evidence is separated from Development windows and persisted under the evaluation research boundary. Identity/replay are deterministic, evidence survives restart, mutated sources are detected, and missing/non-finite holdout evidence fails closed. OOS evidence remains diagnostic and does not promote a model or change execution.

### Feature Ablation v1 — complete foundation

`scripts/analyze_feature_ablation.py` reuses the current 87-feature flattener, purged chronological split and deterministic softmax baseline trainer. It retrains after removing one explicit feature group at a time and reports validation/holdout deltas versus the full model. Cross-group high-correlation diagnostics flag potentially duplicated information. Results are diagnostic only: no automatic feature deletion, Champion promotion, prediction activation or broker effect.

## Evidence and delivery

`docs/development-validation.json` is the machine-readable 0.7 development checkpoint. Promoted 0.6 evidence in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` remains unchanged historical evidence. Documentation-only evidence commits must never be described as newly compiled runtime code.

## Next steps

1. Finish this development state/evidence synchronization after post-merge Research and Windows workflows on `cc0ba8e...` complete.
2. Complete native Windows + Demo MT5 acceptance from `docs/ACCEPTANCE.md`, including native/fallback Sniper latency characterization, fees/rejections/partial fills/restart cases and P50/P95/P99 local timing.
3. Test private Telegram delivery using credentials stored only on the target machine.
4. Run focused Quality Cleanup for verified warnings/deprecations without changing trading behavior.
5. Build historical news/event replay as research-only evidence with immutable observations and causal `first_observed_ms` timestamps; do not use provider `published_ms` alone as proof the system already knew an event.
6. Continue genuine prospective OOS/ablation accumulation across meaningful regimes.
7. Enable required branch checks when repository administration access is available.

Startup/restart is STOPPED. SOLO never starts global execution automatically. STOP blocks new entries while owned timed exits continue with Core active; Full Exit stops client-side timed exits while broker-attached SL/TP remain at the broker. Imported ML challengers remain research-only with no automatic Champion promotion. CI does not establish profitability, guaranteed latency or broker execution quality.
