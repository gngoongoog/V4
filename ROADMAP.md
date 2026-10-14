# BrainTrade OS — Roadmap

GitHub `gngoongoog/V4` is canonical. The broader ARCHITECTURE.md is a design,
not a completed feature inventory. Read BUILD_HANDOFF.md before editing.

## Current source checkpoint

- Historical Meta v1/v2/CES/KTE engines, partial fills and detailed saved ledgers exist.
- Trend/Regime and Structure/Volatility feed a selectable historical Meta v2
  candidate. Candidate gates and engine admission outcomes are persisted.
- Same-data experiments retain their baseline and expose checked cost-aware
  comparisons in the native report and exports. Legacy Meta controls Shadow.
- Offline per-trade candlestick review is implemented; current validation is
  recorded in BUILD_HANDOFF.md and docs/validation.json.
- Windows package delivery remains deferred. Last delivered portable: 0.2.0.

## Next code work — chronological evaluation

1. Add an offline chronological evaluation path over frozen saved History: explicit
   development/holdout or rolling windows; no random split and no automatic tuning.
2. Run Meta v1/v2 on matching windows and costs. Allow warmup before each boundary
   but disallow trades before it; close and reset the ledger at window boundaries.
3. Persist window definitions, input/algorithm identity, metrics and aggregate
   comparisons. Label synthetic, in-sample and held-out results distinctly.
4. Test split boundaries, prefix causality, insufficient data and reproducibility.
5. Keep Shadow on legacy Meta until real-data evaluation is reviewed.

Historical Meta v2 selection, same-data comparison, decision persistence and
legacy-report replay guards are implemented. See BUILD_HANDOFF.md for exact
verification. No real-data effectiveness or profitability conclusion is established.

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
