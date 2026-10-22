# Stage 3F-M: An explicitly invoked read-only exporter for MT5 Demo deal evidence

## Operator-controlled use, never automatic execution

`gateway/demo_evidence_export.py` is an **optional, standalone command-line exporter**. It is not imported by the trading engine and never invokes `order_send`, `order_check`, position operations, account login, credential upload, live-trading controls or background polling. It does not change the real-account execution path. Run it only on a Windows workstation with an already-open MT5 **Demo** terminal and the `MetaTrader5` Python package installed.

It calls `mt5.initialize()` (attaches to the currently configured local terminal) and uses **only read APIs**: `terminal_info`, `account_info`, `symbol_info` and `history_deals_get`. The script rejects Real/Contest/unverified mode, missing history, broker record duplication, unsupported close-by records, invalid economics or timestamps and any detected account switch during the read. It terminates the local MT5 API session with `shutdown()`; it never closes the graphical MT5 terminal.

## Example command — two hours of captured UTC history

Use your intended actual UTC boundaries in *Unix milliseconds*. These numbers are examples, not current market times:

```powershell
python gateway/demo_evidence_export.py --symbol XAUUSD --from-ms 1790000000000 --to-ms 1790007200000 --output demo_gold_deals.json
```

To associate known broker `position_id` values with the synthetic order IDs in Stage 3F-K, optionally supply a **manually verified** local JSON file:

```json
{"123456789":"A","123456790":"B"}
```

and then add `--map-position-ids my_verified_position_map.json`. The script deliberately does **not** guess matching intent IDs from close time, order magic, price, quantity or broker comments. Without a mapping, each `intent_id` remains null/unlinked.

## What the output means

The script writes a local `{ "evidence": {...}, "export_metadata": {...} }` object. For the research API `POST /v1/research/demo-deal-audit`, combine **only** its `evidence` with a separately prepared `bridge` input containing synthetic admission and lifecycle events. Never pass secrets or passwords into the research request. The historical broker deal ticket, position ID, lot size, price, UTC deal timestamp, trade-entry direction and per-deal `profit`, `commission`, `swap`, `fee` are recorded when available.

The data has stronger **local observation provenance** than manually invented deal rows because it reads a Demo session and verifies the account identity both before and after the call. But it remains **operator-supplied**, not cryptographically authenticated by the broker. `evidence.source="caller_supplied_mt5_demo_deals"` and `complete_history=false` **always**. A history window (at most 31 days) might exclude opening or closing fills, or the terminal might still be missing history. Thus the downstream audit correctly reports insufficient evidence unless complete history can be established independently in a future stage.

**This source is not evidence of trading profitability, portfolio risk safety, strategy edge or any live account eligibility.** The exporter does not evaluate filled vs missed market orders, price slippage, order_check, server rejection codes, close-by/netting, FX translation, unlinked fees, pending orders or incomplete terminal history.

The exporter fails closed for unexpected close-by entry codes or unsupported histories rather than silently counting them as exits. Its unit tests under `tests/test_demo_evidence_export.py` use a fake MT5 object and cover Demo-only authentication, before/after account switch, missing/empty records, unique deal ticket, validation of timestamp/symbol/mapping, no order_send and unlinked intent handling. Run exact-head Windows/Research/Linux CI before merging.
