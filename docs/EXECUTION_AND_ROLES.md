# BrainTrade OS 0.6.0 — broker execution and membership

The bound Demo MT5 account can receive actual orders when global START is active. Initial startup/restart is STOPPED; execution is initially unbound. The account type is checked against the broker: Demo=0, Live=2. Live must be selected and rebound explicitly while stopped and without unresolved intents or owned positions. Changing accounts never silently transfers ownership.

## Coordinator and independent brains

The Execution page has 14 identities: system, Gann, Wyckoff, trend, momentum, structure, supply/demand zones, broker DOM, activity profile, Gann+DOM+profile, CES, KTE, regime and a manually selected generated strategy. Each has its own configuration, magic number, signals, broker positions, recorded deals and results. CES/KTE v1 use M1; the other secondary policies can select the available native frames. News and volatility are analysis/context components, not separate execution strategies.

Secondary brains are advisors by default. The SOLO button removes that brain from the actual coordinator vote and enables its own policy to submit orders independently of other brains. It does not press global START. Return disables new independent entries. An outstanding intent or owned position puts the brain in RETURNING: it keeps managing its original position and rejoins the coordinator only after settlement. SL/TP and the recorded maximum holding time remain tied to the entry. STOP prevents new entries; timed exits and broker SL/TP remain active. Full application exit stops client-side timed management; broker-attached stops remain at the broker.

A membership/configuration change cancels queued entries and changes a persisted revision. Dispatched entries must match the revision at final authorization; already submitted orders cannot be undone. All brains use the one master START/STOP. The system identity is always the coordinator, and has no SOLO switch.

## Adaptive coordinator

Expert uses the actual non-SOLO, non-returning member signals. Before 20 scored observations a member's weight is 1. Subsequently the weight is `0.5 + (wins + 10) / (samples + 20)`, bounded to 0.5–1.5. BUY/SELL weights are summed; normalized imbalance at least +0.20 or at most -0.20 selects a direction, otherwise WAIT. Other modes retain their own trigger, vetoed when opposing weight is at least 2 and exceeds supporting weight by more than 0.5. The Execution detail shows participating members, weights, sample counts and reasons.

One prospective observation per member/frame/bar is recorded while START is active. Its outcome uses exactly 12 later completed bars, frozen entry quote/contract/commission/slippage and positive historical exit spread where available (otherwise frozen entry spread). Missing retained horizons expire; they are never invented. Samples are isolated by account/server/symbol/currency, mode, brain, frame and generated-report ID and persist in SQLite. Weighting is a smoothed observed win frequency, not calibrated confidence, deep learning or a profitability claim. Mean net per lot is also reported; magnitude is not used by this weighting formula. Swaps are excluded from these learning labels. The older imported ML challenger pipeline remains research-only and has no automatic promotion.

## Order transport and limits

The Core persists intent before dispatch. The gateway journals each command before `order_send`, checks the exact account, broker contract, owned ticket, quote and terminal permissions, calls `order_check`, then requests final Core authorization. Repeated IDs, timeouts, unknown replies and accepted-but-unobserved fills never cause a blind resend. Unknown intents remain blocked until broker reconciliation; broker-modified comments may prevent automatic reconciliation. No top-up is attempted after partial fills. Exit requests use actual remaining owned volume. Netting accounts serialize symbol ownership; separate simultaneous brain positions require a hedging account.

Optional lot/risk/daily loss/drawdown/spread budgets default OFF per brain, as requested. Enabling them applies the selected settings. Drawdown uses the persisted highest observed equity for that account/symbol; deposits/withdrawals are not normalized. One active position/unsettled intent per brain, lot-step validity, ownership, account identity, quote freshness, stop geometry, START and duplicate prevention are execution correctness requirements. Strategy conditions and SL/TP still apply. Generated strategies require a manually selected saved report from the same account, symbol and timeframe; no automatic promotion based on research performance.

Broker deal profit, commission, swap and fee are recorded and deduplicated by ticket. Manual exits with magic zero are attributed by the known position identifier. Fees without a usable position ID cannot automatically be assigned to a brain. The calculator's commission is a round-trip estimate per full lot in account currency; actual results use broker-reported fees.

The gateway requests 100 ms between execution cycles. This is a polling sleep, not a promised decision/order latency. Account/history calls, analysis, network and broker work add time. Sniper evaluates a live bid/ask breakout of the last six completed M1 bars with M5 agreement; it does not wait for the next candle close. Target and stop use broker points, with commission/exit slippage compensation in the target. The selected maximum cost fraction also applies to the actual Sniper broker policy; a cost-heavy entry remains WAIT. Broker-attached SL/TP can act independently of UI polling. Decision microseconds and queue/check/send milliseconds are measured on actual operations. The dashboard refresh interval is separate. This is not an HFT or tick-complete feed.

## Persistence and verification

Backup schema 12 includes configurations, broker intents/deals/observations and adaptive samples. The separate durable gateway send journal is intentionally never rolled back with an application backup, so restoring an old Core database cannot resend an already journaled order. A restored/new Core starts STOPPED. Existing working databases migrate additively; sealed backups require the exact application version/schema.

`python scripts/check_execution.py` runs a real Core against a simulated MT5 object in isolated temporary storage. `--exe BrainTrade.exe` checks the packaged Windows Core, including DPAPI IPC. It is not native broker acceptance. Latest recorded CI evidence belongs in `docs/validation.json`.

Official transport references: https://www.mql5.com/en/docs/python_metatrader5/mt5ordersend_py and https://www.mql5.com/en/docs/python_metatrader5/mt5ordercheck_py.
