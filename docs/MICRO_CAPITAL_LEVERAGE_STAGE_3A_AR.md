# V4 Micro Capital / Leverage Preview — stage 3A (2026-10-22)

## Scope and account-sized behaviour
This is an isolated, read-only extension to the existing v0.7.4 broker-verified stop-risk cap. A user may type a **scenario leverage** such as 500 (1:500) in Risk. It does **not** configure real leverage, change order volume, bypass Core risk checks, or create executable orders. A real broker account's leverage must be changed at the broker.

The MT5 gateway now reports broker-authoritative `account_info().leverage`, `margin_free` and `order_calc_margin(BUY, symbol, volume_min, current ASK)`. Missing/invalid inputs remain absent; no fabricated values. The dashboard displays actual leverage and a deliberately **indicative** inverse-leverage margin comparison for the minimum broker lot. This simplistic comparison is **not a margin guarantee** for fixed-margin gold CFDs, tiered margin, hedged accounts, account currency or broker-specific requirements. Real trades continue to rely on MT5 `order_calc_margin`, broker min/step, Core authorization and the broker's own acceptance.

For a **$100** account the micro diagnostics surface whether even a minimum lot is plausibly margin-affordable; **risk is a separate gate**. The existing broker sizing computes `stop-loss loss + full-roundtrip commission + two-side slippage`, and rejects when that loss exceeds `equity × risk_percent / 100`. When the minimum lot does not fit, the rejection now also states the **percentage of equity needed** and explains that leverage cannot reduce a given lot's stop-loss exposure. This percentage is diagnostic, **never an automatic risk override**.

`commission_per_lot=7` remains exactly `$0.07` for 0.01 lot, `$0.70` for 0.10 lot, and `$7` for 1.00 lot, round trip; zero is allowed when the broker is configured for no commission. All prices and contract PnL remain broker-dependent.

## Acceptance checkpoints
- Python: `tests/test_gateway.py` verifies real leverage and minimum-lot margin collection; `tests/test_broker_execution.py` verifies micro-account rejection diagnostics without an unauthorized send.
- Rust/Slint: telemetry fields are optional/backwards-compatible; desktop Risk page displays a connected account's actual leverage and updates the illustrative comparison from the entered scenario.
- Run **Research and Windows/Linux CI against the exact PR head** before merge. No claim of real broker fill validation until the user's Windows MT5 Demo acceptance.

## Planned stage 3B (not implemented here)
Evaluate bounded small-account risk presets and more explicit portfolio risk sizing with scenario-specific tests. Never silently raise risk over the user's selected budget, invent sub-minimum lots or narrow/widen strategy SL to force a trade. Add independent daily loss circuit breaker and externally owned cross-symbol position accounting in later separately tested stages. Keep startup STOPPED and explicit REAL confirmation gates.

## Limits
Changing the scenario from 1:100 to 1:500 will not alter the broker account or guarantee a cheaper margin. Broker leverage and symbol contract may impose additional restrictions. Stop loss is not a guaranteed fill price. No profitability/latency/real-money readiness claim.
