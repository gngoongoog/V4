# Student learning — implementation and acceptance

This iteration extends Flexible Demo in V4 development. It does not change the 16 execution magic identities or automatically operate a live account.

## What the system learns

1. An acknowledged Demo entry freezes its account, symbol, mode, advisor, timeframe, report, market regime, UTC session, spread/ATR context, entry reason, selection probability, geometry, commission and contract values. Exact closed-position labels come from reconciled MT5 deals, including booked commission, swap and fees. Partial history cannot become a closed label.
2. Advisor weights receive a bounded contextual adjustment from results matching regime, session and spread/ATR bucket. Sparse evidence shrinks toward the existing global/regime estimate. Sources remain isolated by account, symbol, mode, brain, timeframe and generated report.
3. Flexible Expert samples **ready** advisors using epsilon-greedy selection. Epsilon is `clamp(0.30 / sqrt(1 + broker_samples / 20), 0.05, 0.30)`. The best candidate has probability `1 - epsilon + epsilon / candidates`; others have `epsilon / candidates`. Stable seeded draws preserve repeatability for unchanged opportunities. This probability is a policy-selection probability, not predicted profit. No random entry direction is created. Independent SOLO execution remains unchanged.
4. Each acknowledged entry starts three quote-only exit trials. Baseline: original stop/target/hold. Quick: 0.90× stop, 0.75× target, 0.50× hold. Patient: original stop, 1.25× target and hold. Shadow long exits use Bid and short exits use Ask; observed prices include level overshoot. They subtract configured round-trip commission and exit slippage from the actual acknowledged entry price. Entry spread and actual entry slippage are already present in that price and are not charged twice.
5. A persistent study compares bounded net R on the same entry paths. After 20 usable pairs, it freezes the highest-mean challenger. Validation only accepts entries opened after this freeze. A further 20 pairs must have positive candidate net R and paired mean gain minus two estimated standard errors above zero. This is a promotion heuristic, not a calibrated confidence statement. Twenty later monitoring pairs trigger rollback when the gain or candidate result becomes non-positive.

## Execution and scope

- A learned Sniper target is checked against the configured cost fraction. If it no longer fits current spread/commission, that entry retains its already-valid baseline geometry; the journal explains the fallback.
- Automatic exit application is enabled by default on Demo, independently configurable under Trading → Mode and costs. Existing open positions retain their frozen geometry and hold time.
- Source identity includes the mode, brain, timeframe, generated report, strategy geometry definition, Sniper settings, configured commission/slippage and tick size. Changed definitions start a separate study. Monetary tick values are frozen per experience for cost/R calculation; conversion-rate changes do not reset the strategy study.
- STOP, account identity, broker volume constraints, ownership, reconciliation and the durable no-resend journal remain authoritative. Learning sample counts never block a valid entry.
- Fast pulses only update in-memory quote trials. SQLite writes and policy changes happen on regular snapshots. Only complete actual closes can admit their paired trials to a study. Shadows never write broker PnL or broker commands.
- Experiences and studies survive restart and are included in SQLite/Brain Snapshot backups. Account switching cannot reuse another account's studies.

## Dashboard

Trading → **Learning progress** is page 20. It shows experience counts, exact broker closes, usable paired trials, patterns in the latest 100 experiences, study stages, active/candidate profiles, last comparison, promotion/rollback counts, and the newest 20 detailed entries. Trading → Mode and costs controls automatic Demo exit application.

Patterns include cost drag, adverse observed movement and profit giveback. They describe repeated observations and do not establish why a trade lost. MFE/MAE are extrema from observed positions, not tick-perfect lifetime extrema. Short trades may close before the five-second broker-history sync exposes their identifier; such trades still supply booked net outcomes but may have limited path coverage.

## Evidence limits

- Quote-path gaps over two seconds invalidate comparisons, without blocking trading. Trials without sufficient observations expire explicitly. Broker acknowledgements provide local entry time; this is not exchange fill time.
- Shadow comparisons estimate execution at observed executable quotes; they do not reproduce MT5 stop execution, market impact, liquidity, swaps, or unobserved ticks. The real booked outcome remains separate.
- Conservative coverage requirements may leave long-hold modes collecting evidence for a long time. The original exit stays active throughout. No fixed number of trades guarantees an improvement or profitable trading.
- The initial implementation learns advisor preference and these bounded exit variants. It does not rewrite arbitrary strategy code or establish causal explanations from a single win or loss.

## Verification

Local and CI gates cover domain/runtime tests, Python gateway tests, the Core + simulated broker integration, and native dashboard render checks at 1120×820 and 880×700. The integration checks a $100 simulated account, actual adapter `order_send` invocation, margin downsizing, duplicate protection, complete/partial history, selected-advisor attribution, contextual evidence, journal persistence and account isolation.

Domain checks cover side-correct prices, costs once, missing-path exclusion, stable exploration, chronological training/validation, promotion and rollback. Runtime checks cover frozen future-entry profiles, config/account/live boundaries, real-label separation and restart idempotence. Native MT5 broker acceptance and any claim about real profitability still require the user's Windows Demo run.
