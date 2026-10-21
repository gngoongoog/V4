# BRAINTRADE V4 — Pre-Live Operational Readiness (21 October 2026)

## Current state (do not confuse with real-broker certification)
Version 0.7.1's full codebase has previously passed automated Rust, Python, native Slint Windows builds, core simulated-MT5 broker workflows, offline broker journaling, and research validation. The exact changes in this PR require their own matching GitHub Actions success. **No external/real MT5 broker account has been accessed or traded during this audit.** A fully automatic trading system cannot truthfully be certified “100% ready” from source code alone, nor can positive Demo profit guarantee future live profit.

## Verified execution architecture
- Native Windows UI and Core are separate processes; Core owns persistent binding and local API authority. A typed REAL account login is **not a login credential**; it is a one-time high-friction consent control.
- Mode choice: Scalping / Swing / Expert / Sniper. The selected system coordinator alone owns new MT5 entries; other brains are advisor or Shadow/Paper observation, not independent broker senders. Broker-side final account/mode checks happen immediately before real `order_send`.
- Selected broker `demo` needs actual MT5 trade_mode `0`, `live` needs trade_mode `2`. Login, server and currency must match the persisted binding. Core restarts STOPPED. The trading target is not changed merely by switching MT5 accounts.
- NEW: a REAL *bind* additionally requires typing the exact currently connected account login in the Execution page and fresh telemetry. That confirmation is not persisted.
- NEW: every REAL *START* independently requires entering the bound real account login again, a fresh matching trade-enabled MT5 account, and enabled Core portfolio exposure protection. Rejecting a START does not arm the system.
- Demo START continues allowing monitoring and simulations without any REAL confirmation. STOP cancels queued entry intents; it does **not** liquidate open positions. Bracket stops and position exits continue to require the Core/MT5 process alive and broker acceptance.
- Native Live permission is still not equivalent to proven model profitability or full broker compatibility.

## Learning and reporting truthfulness
- Sniper learner and coordinator use explicitly scoped, verified Demo broker fills and separate causal-forward observations to update decision evidence. Shadow/Paper results are approximate sampled quotes and do **not** automatically train real-execution weights.
- Real broker journal attributes booked PnL by matched MT5 position/deal history and separately reports commission, fee and swap; unmatched historical entry reasons and unresolved partial closes are marked unverified.
- For exact XAUUSD TP, candidate pips are mapped into explicit price distance. A broker that cannot represent the target on its tick grid or stops level must reject it rather than silently widen to a materially different TP.

## Critical live acceptance checklist — must be executed on the operator's real Windows MT5 Demo terminal before funding
1. Keep target on Demo; verify MT5 shows the expected login/server/currency and allowed Algo Trading status. Save the chosen trading mode, broker symbol, lot and commission setup. Calibrate the 1-pip gold price conversion against the broker's actual XAUUSD contract.
2. Enable Core portfolio limits and confirm exposure across existing managed and externally opened trades. Configure conservative max positions, max lots and maximum estimated stop risk for the chosen broker. Do not assume risk_percent or daily loss stops guarantee executions during gaps or slippage.
3. Place and fully close a bounded set of Demo trades with the exact entry strategy, stop, TP, time stop, fees and fill ticket recorded; compare with MT5's own trade-history records (position_id, deals, partials and commission).
4. Exercise forced broker disconnect, quote staleness, STOP, reconnect, unknown/partial sends, switching MT5 logins while STOPPED, and subsequent recovery. No duplicate send, wrong-mode live order or fabricated PnL should occur.
5. Test strategy selection after restart and ensure no account cross-contamination of learned weights, Paper data or permissions. Confirm no independent advisor opens.
6. Run walk-forward and out-of-sample testing with actual tick/spread history and execution quality. Assess both absolute and fee-adjusted profitability, max drawdown and catastrophic-loss tolerance. No model is guaranteed to win.
7. Before REAL: STOP; close/reconcile old strategy-owned positions. Log into actual MT5 real account intentionally and verify `REAL` in MT5. Select Live in UI, enter account number to bind. Confirm portfolio protection is ON; enter same number independently on the top START control. Start at the broker's minimum practical risk only if Demo acceptance passed.
8. In REAL verify journal, booked net, current slippage and kill-switch behavior immediately. STOP prevents **new** entries; open risk continues. A close, broker SL and a human emergency plan are still required. Real PnL may deviate substantially from Demo.

## Explicit not-yet-verified claims
- Broker-specific MetaTrader 5 Demo/REAL fills, partials, swaps, true live commissions, end-to-end subsecond latency, native EA compatibility on the operator's machine, and Telegram notifications are **not validated here**.
- Profitability and safe continuous unattended REAL operation are **not established**. The UI's research output cannot be treated as a certified decision brain.
- “100% production-ready” is not a testable or supportable claim without verified on-machine behavior, broker acceptance and risk-managed operation.
