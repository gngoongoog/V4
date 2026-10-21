# BRAINTRADE V4 — Execution, Learning and Journal Integrity Audit (2026-10-21)

## Scope and honesty boundary
Reviewed the promoted 0.7.1 main source tree (0904dd883ab2), including Rust domain coordinator and Sniper quote signals, runtime broker order authorization, MT5 Python gateway, broker deal-based learning, shadow/paper ledger, native brain-control and trade-journal pages, and Arabic operator documentation.

This is a **code/integration audit, not an MT5 broker certification**. No real account, broker feed, filled trade, MT5 terminal, Telegram message, sustained out-of-sample profit or native OnTick latency has been observed during this audit. Shadow outcomes are approximations from sampled bid/ask; they cannot establish live execution expectancy.

## Confirmed defects fixed in this branch
1. **Cross-mode veto:** `domain::execution::coordinate` allowed slow secondary advisors to cancel the actual Scalping, Swing or non-learner Sniper signal, particularly on Live or when Demo flexibility was off. Non-Expert votes are now diagnostic-only; Expert is still the only consensus/selection mode. Added Demo/Live, all three modes, stop/target integrity regression coverage.
2. **Weak Sniper entry evidence:** Quote impulses could be triggered by as little as two broker ticks of movement, given the previous 0.5 efficiency threshold. Candidate impulse/rebound now require an excursion relative to both broker tick size and live spread, directional efficiency, and a real prior excursion before reversal; reason logs show measured movement and costs. These thresholds may reduce frequency. Added tiny-quote-noise tests; the numerical thresholds themselves need proper out-of-sample validation, not a claim of profitability.
3. **False ownership attribution:** For an open MT5 position whose opening deal has not yet synced, the UI journal previously copied the most recent open command of the same magic identity, possibly from *another position*. It now requires exact broker position identity to link a command, otherwise displays `UNVERIFIED_OPENING_STRATEGY` while retaining magic-verified ownership. Added unrelated-entry regression.
4. **Partial-close false positives:** A broker history containing a partial closing deal but no verified open-position snapshot was previously labeled `CLOSED`. A fully closed designation now requires full covered opening volume (without ambiguous reversal) or an authoritative connected MT5 position snapshot with no remaining position; otherwise displays `EXIT SEEN / CLOSE NOT VERIFIED`. Added partial-close regression.
5. **Misleading Paper wording:** Paper-price-sampled outcomes were easy to mistake for model-weight training. The brain control UI and operator guide now clarify that Paper is **simulated performance observation**; those outcomes do not automatically update the real coordinator weights. MT5 Demo booked trade evidence and separately labeled causal forward evidence feed the learning paths.

## Existing safeguards reviewed (not weakened)
- Only `system` owned by the selected mode can send *new* MT5 entry intents; previously owned exits remain authorized for lifecycle management.
- Final Core authorization and MT5 account/permission recheck happen *after* the durable gateway send journal, before `order_send`; uncertain sends are reconciled rather than retried blindly.
- MT5 fees, actual Bid/Ask spreads, broker stop distance and fixed Sniper TP geometry remain distinct.
- Sniper policies reload from account/context-scoped **closed Demo broker outcomes**, with target-arm credit gated on preserved exact TP.
- Read-only native Trade Journal and mode/account scoped brain control do not grant additional broker order permissions.

## Verification matrix
- Existing last-main Windows/Linux CI for main commit `0904dd88` succeeded; exact head for this audit branch must be retested before merge.
- The provided portable source reproduces 150 Python unit-test cases locally; the one subprocess test requiring the unbuilt Linux `braintrade-core` binary cannot run in this container. After disabling Python bytecode file writes, other tests pass with one platform skip.
- New Rust tests require GitHub Actions CI because a Rust toolchain and a native MT5 terminal are unavailable in the current container.
- Real MT5 Demo and Arabic Telegram end-to-end acceptance remain required on the operator's configured Windows broker machine; no unconditional production-ready statement is justified.

## Operator Demo acceptance criteria still outstanding
1. Match each new Sniper/Scalping/Swing entry to its selected-mode signal and exact journal request, including TP, SL, commission and broker price/quote timestamp.
2. Test START, STOP, rapid mode changes, pending/unknown sends, partial fills, netting/hedging and account switch while verifying no duplicate or wrong-mode opens.
3. Compare round-trip commission, spread, swap, fee and position_id match with MT5 history, including partial-close and offline/reconnect cases.
4. Verify that closed Demo outcomes increment corresponding scoped model evidence and that Paper-only outcomes do not contaminate real broker weight.
5. Test forward chronological out-of-sample behavior and execution latency with the user's real spread/slippage environment; profitable expectancy is unproven.
