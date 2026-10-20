# Native Windows + MT5 acceptance — pending

These checks require a Windows x64 host with a real MT5 installation and a demo account. GitHub Linux/Windows CI does **not** represent these checks as passed.

## Core / gateway / desktop

- Run the validated portable from a path with spaces and Arabic characters; no terminal window required.
- Use the native file picker to select the actual `terminal64.exe`.
- Sign into the intended demo account inside MT5, close MT5, launch BrainTrade and verify the selected terminal/account is reopened/read correctly.
- Match balance/equity, positions and pending orders, including zero-position state.
- Force read failures/disconnects; unknown state must not become a fabricated empty/current snapshot.
- Switch account during collection; a mixed account snapshot must be rejected.
- Close/reopen desktop; Core/audit continuity should remain correct. Full Exit must stop Core/gateway while leaving MT5/broker positions untouched.
- Verify Arabic shaping, file picker, resizing/scrolling and common Windows display scaling.

## Live research / Shadow

- Match completed M1/M5/M15 broker candles and freshness behavior.
- Confirm stale/missing/disagreeing inputs show WAIT/withdrawn evidence rather than synthetic data.
- Verify Start/Stop semantics and persisted virtual Shadow state after Core restart.
- Confirm the selected mode drives Shadow; imported ML challengers remain research-only. Broker coordinator results are separate from this virtual ledger.
- Confirm actual MT5 positions/orders veto virtual entries where designed. With execution unbound or STOPPED, no new broker entries should be sent. Bound START can submit broker orders.

## Historical research

- Fetch short real broker-history ranges for Meta v1, Meta v2, CES and KTE.
- Verify exact account/server/symbol/contract identity, incomplete history/gap reporting and saved frozen assumptions.
- Replay saved runs offline and confirm deterministic exports/reports for unchanged inputs.
- Check KTE fills/targets/stop updates and HTML candlestick review against saved ledger data.
- Run same-data Meta experiments and verify comparison ancestry/source identity.
- Run chronological Meta evaluation and verify persisted window/source identity survives restart.

## Learning research

- Collect enough real completed-bar snapshots and allow exact 5/15/30-minute labels to mature.
- Verify missing/gapped future horizons remain unlabelled rather than shifted.
- Export a dataset and independently verify its dataset identity before registering any offline-trained challenger.
- Confirm manual review eligibility requires matching chronological evidence for all contexts.
- Confirm read-only inference consumes stored features and produces audit predictions without changing Shadow.
- Confirm predictions remain pending until the exact target label matures, then produce one immutable scored outcome and descriptive metrics.
- Confirm no champion activation/promotion API exists and `promotion_enabled`, `shadow_effect`, `broker_effect` remain false.

## News / Telegram / secrets

- Verify official Fed/ECB source/freshness behavior and stale-cache handling.
- Configure Telegram token locally only; inspect config/audit/backups for absence of plaintext secret.
- Verify only the authorized private user/chat receives read-only information and live token changes do not leak obsolete responses.
- Validate Telegram delivery with the actual network/account separately from unit tests.

## Backup / persistence

- Create backup while Core runs and verify SQLite integrity and expected manifest fields.
- Confirm secrets are excluded.
- After STOP, validate and stage a backup, confirm replacement, reopen the app and verify the result.
- Confirm rollback snapshot exists, local Telegram/IPC secrets remain unchanged, and corrupt or incompatible backups are rejected.
- Restart and verify historical/evaluation/learning audit state survives.

## Demo broker execution and roles — manual acceptance still pending

- Bind the intended Demo login/server/currency, confirm Algo Trading is permitted in MT5 and start the system.
- Verify a coordinator order appears in MT5 with the recorded magic, requested volume and SL/TP; compare the actual broker result and fees with the Execution ledger.
- Select an advisor, press SOLO and verify exclusion from the coordinator member list; its own trade must use its own magic and settings.
- Return it with an open position. It should show RETURNING, continue its original exit management and rejoin only after settlement. Changing membership must not turn START on.
- Verify STOP prevents new entry, while timed exits and broker SL/TP retain their ownership. Full Exit stops client-side timed exits.
- Exercise broker rejection, partial fill, disconnection and restart on Demo; an uncertain result must never cause a blind duplicate submission.
- Compare actual fee entries, swaps and manual exits against position-attributed totals. Fees without position IDs need separate accounting.
- Verify configured commission proportionality, actual spread and Sniper cost threshold.
- On hedging accounts, verify separate brain positions. On netting accounts, verify exclusive ownership of the symbol.
- Verify all four modes. Swing needs actual H1/H4/D1 history; Sniper needs a fresh tick crossing the completed-bar range. WAIT without a valid strategy signal is expected.
- After enough prospective member outcomes mature, verify sample counts/weights survive restart and remain isolated by account/mode/frame.

## Sniper fast-lane latency — target-machine characterization required

- In Sniper mode confirm the gateway hot loop is active while full account/position reconciliation and research/history collection remain on their slower schedules.
- Collect at least several hundred Demo execution samples and preserve `core_pulse_ms`, `queue_to_gateway_ms`, `authorize_ms`, `journal_ms`, `post_journal_authorize_ms`, `post_journal_account_ms`, `other_pre_send_ms`, `pre_send_ms`, `queue_to_send_ms`, `send_ms` and `total_ms` from broker results.
- Compute P50/P95/P99 separately for `queue_to_send_ms` and `send_ms`. The engineering target is **20 ms or less for BrainTrade-local queue-to-send latency** on the target machine; this is not a broker-fill or network guarantee.
- Confirm tick-keyed Sniper entries report `fast_path=true` and `order_check_skipped=true`; non-Sniper/bar-based entries must keep the normal broker `order_check` path.
- Verify `PRAGMA synchronous=FULL` journal persistence still completes before every `order_send`; low latency must not permit a blind duplicate after crash, timeout or uncertain broker acknowledgement.
- Verify the post-journal Core authorization rejects an Open if STOP or execution revision changes after the durable journal was committed, and verify the final account read rejects a send if login/server/currency/Demo identity changes in that same last-moment window.
- Compare actual `post_journal_authorize_ms` and `post_journal_account_ms` cost against the other local components. They are intentional safety gates and must be measured before considering any latency redesign.
- Compare fresh quote time, command creation time and the final `order_send` result. If local P95/P99 exceeds the target, use the component timings to identify whether Core authorization, durable journal I/O, final authorization/account validation or local MT5 calls dominate before changing any safety boundary.
- Treat MT5/broker acknowledgement and fill latency separately. No fixed end-to-end latency may be claimed from CI or from the local fast path.

## Native Sniper OnTick / CopyTicks bridge — target-machine acceptance required

- From the portable package, copy `MT5/Experts/BrainTradeSniperBridge.mq5` into the selected terminal's `MQL5/Experts` folder if it is not already there, compile it with that terminal's MetaEditor and attach it to the exact broker symbol used by BrainTrade (for example the broker-specific XAUUSD name).
- Start BrainTrade first so it creates the short-lived session token under that terminal data folder at `MQL5/Files/BrainTrade/sniper.token`; the EA should then show `BrainTrade Sniper Native: CONNECTED` on its chart.
- Confirm the EA contains no trading API and does not place, modify or close any order by itself. Broker submission must remain exclusively in the journaled Python execution adapter.
- During a fast Demo quote burst, compare the EA's quote-tick counter with a `CopyTicks(..., COPY_TICKS_ALL)` capture over the same fresh interval. Pure Last/volume-only ticks may be consumed by the cursor but only Bid/Ask changes should be forwarded to Sniper decisions.
- Verify several Bid/Ask changes with one `time_msc` are still evaluated as distinct quote events rather than collapsing to one `tick:<ms>` decision.
- Detach or stop the EA while Sniper is active and confirm the gateway automatically falls back to the 5 ms Python quote polling path without enabling execution by itself.
- Reattach the EA and confirm native wake resumes without restarting Core and without creating a duplicate broker command for a previously observed quote.
- Disconnect/reconnect MT5 for less than one second and confirm cached fresh quote changes are recovered in source order. After a longer gap, confirm the bridge resumes from the current millisecond instead of replaying stale history into live decisions.
- Force a pipe interruption/partial write if possible and confirm the EA does not advance its source cursor until the whole ASCII batch is written; the next OnTick should recover that batch from MT5's tick cache.
- Verify an invalid token, wrong login or wrong symbol cannot update the reconciled BrainTrade quote state.
- Record native-path P50/P95/P99 separately from fallback-polling P50/P95/P99. Treat this as local reaction characterization only; neither path proves a fixed broker acknowledgement/fill latency or guaranteed capture of every upstream broker event.

## Portfolio Exposure v1 acceptance

- Portfolio Exposure v1 is implemented and OFF by default; confirm upgrading or restarting does not silently enable it.
- While STOPPED, enable it and configure aggregate maximum positions/intents, lots and estimated stop-risk percentage.
- Verify new entries that would exceed any enabled portfolio limit are cancelled while existing positions, broker SL/TP, timed exits and reconciliation continue.
- Verify unknown aggregate stop risk fails closed for new entries rather than being treated as zero risk.
- Confirm settings survive restart and backup/restore according to the current schema.

## Separate future work

Cross-version/multi-account migration, correlation-driven broker execution policy and imported ML champion activation are not implemented. Aligned multi-symbol correlation/exposure **research** remains a separate evidence step before any correlation-driven execution policy. Existing working databases migrate additively; sealed backup restore is restricted to the same app version/schema.