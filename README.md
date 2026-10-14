# BRAINTRADE OS — 0.4.0 Preview

Canonical source: `gngoongoog/V4`. `development` contains active research work; `main` remains the last deliberately promoted stable/resumable checkpoint.

Current release candidate: **0.4.0 Preview**. Cargo, lockfile, desktop and Windows package versions are aligned. The user has requested Windows delivery; use BUILD_HANDOFF.md and docs/validation.json for the exact final validation boundary.

## Windows launch

Extract the complete portable ZIP and open **BrainTrade.exe**. Python and the gateway
are bundled. In Settings, choose the installed MT5 `terminal64.exe`, save the exact
broker symbol and reconnect. See `docs/QUICKSTART_AR.md` for the Arabic guide.

The dashboard has 13 pages, including dedicated Opportunities, Market Map and
Learning/review surfaces. No separate server command or Python installation is
needed by the end user.

## Implemented now

- Native Rust/Slint desktop plus an independent Rust Core and authenticated loopback IPC.
- Read-only MT5 gateway for account state, positions/orders, completed candles, historical bars and broker-supported Market Book depth.
- Start / Stop / reconnect / full-exit controls; persistent rule-based Shadow simulation and risk limits.
- Historical Meta v1, Meta v2 candidate, CES v1 and KTE v1 with frozen inputs, saved reports, offline replay and same-data experiments.
- KTE partial TP1/TP2/TP3 ledger, break-even/trailing updates, conservative OHLC ambiguity handling and offline candlestick trade review.
- Observational market brains/opportunities for Trend, Regime, Structure, Volatility, Gann geometry, broker DOM depth and Macro/News context.
- Persisted chronological Meta v1/v2 evaluation over immutable saved history.
- Learning Foundation: immutable completed-bar snapshots, delayed exact 5/15/30-minute labels, deterministic dataset export, offline linear-softmax baseline trainer and immutable challenger registry.
- Manual challenger review-eligibility records, read-only challenger inference using the exact stored feature payload, and matured prediction outcome scoring with descriptive out-of-sample metrics.
- Official Fed/ECB RSS cache and private read-only Telegram query handlers.
- Backup metadata includes research/learning audit state through schema 8; full restore is not implemented yet.

## Non-negotiable safety boundary

Legacy rule-based Meta remains the live Shadow entry source. Learned challengers are audit telemetry only. There is no champion-promotion endpoint and no broker execution route. The Python MT5 bridge does not call `order_send`, modify or close broker positions. Historical/ML metrics are not profitability guarantees.

## Validation and quality

`docs/QUALITY_AUDIT.md` records the repository-wide consistency review, packaging hardening, SQLite foreign-key enforcement and permanent contract tests. `BUILD_HANDOFF.md` and `docs/validation.json` record the exact tested code checkpoint and CI runs.

Native acceptance on the user's real Windows/MT5 environment and live Telegram delivery remain separate manual checks; CI does not claim those passed.

## Resume in this order

1. `BUILD_HANDOFF.md` — exact tested checkpoint, remaining limitations and next action.
2. `PROJECT_STATE.md` — implemented vs pending state.
3. `ROADMAP.md` — ordered next work.
4. `docs/QUALITY_AUDIT.md` — integrity/quality findings and open limitations.
5. `CHANGELOG.md` — completed milestones.

## Developer verification

```sh
cargo test -p braintrade-domain -p braintrade-runtime --locked
cargo build -p braintrade-runtime --bin braintrade-core --locked
python3 -m unittest discover -s tests -v
python3 scripts/smoke_core.py
cargo check -p braintrade-desktop --locked
cargo run -p braintrade-desktop --locked -- --render-check
```

Do not commit Telegram tokens, API keys, broker passwords/credentials, runtime databases/logs, generated dependency trees, `.import`, or source ZIP/chunk staging artifacts.
