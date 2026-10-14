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
- Confirm live Shadow entries still come from legacy Meta and observational Gann/DOM/Macro/Meta-v2/Learning outputs do not change Shadow action, risk or lot sizing.
- Confirm actual MT5 positions/orders veto virtual entries where designed and no broker order is sent.

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
- Confirm the application does not falsely present full restore as implemented.
- Restart and verify historical/evaluation/learning audit state survives.

## Still future / separate acceptance

Broker execution, hard execution risk, reconciliation, partial live fills, portfolio execution risk, full restore/migration and any champion activation require separate designs and acceptance plans. None are enabled by the current research checkpoint.
