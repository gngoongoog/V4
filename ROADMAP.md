# BrainTrade OS — Roadmap

GitHub `gngoongoog/V4` is canonical. The broader ARCHITECTURE.md is a design,
not a completed feature inventory. Read BUILD_HANDOFF.md before editing.

## Current source checkpoint

- Historical Meta/CES/KTE engines, partial fills and detailed saved ledgers exist.
- Trend/Regime, Structure/Volatility and Meta v2 candidate are implemented as
  observational outputs. Legacy Meta still controls Shadow entries.
- Offline per-trade candlestick review is implemented; current validation is
  recorded in BUILD_HANDOFF.md and docs/validation.json.
- Windows package delivery remains deferred. Last delivered portable: 0.2.0.

## Next code work — evaluate Meta v2

1. Add a separately versioned selectable historical Meta v2 path.
2. Compare it with legacy Meta on identical frozen histories and costs.
3. Test causal prefix stability, missing/stale data, gaps and risk behavior.
4. Persist candidate gates/decisions for review; preserve old report replay.
5. Keep live Shadow on legacy Meta until the evaluation is reviewed.

## Laptop acceptance (when available)

- Native Windows UI and MT5 historical retrieval against the user's broker.
- All pages and supported analysis timeframes.
- Telegram delivery after credentials are configured locally.

## Later research

- Gann, actual feed-supported order flow/DOM, volume profile.
- Historical news/event context and replay.
- Walk-forward evaluation, model/experiment registry and trained ML/ONNX.
- Portfolio/correlation risk and full restore/recovery workflow.

## Execution, after explicit validation

- Broker order state machine, idempotent intents, reconciliation and protections.
- Partial execution/reconnect recovery fixtures and account-mode handling.

A module is complete only when code, verification and state documentation agree.
