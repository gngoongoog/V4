# Meta Brain v2 — historical candidate v1

Meta Brain v2 remains observational in live analysis and is selectable as a historical candidate. It is visible in the existing brain analysis output but does not replace `Analysis.action`, does not change Shadow entries, and has no broker execution authority.

Inputs and gates:

- Trend Brain v1 supplies aligned direction and normalized trend evidence.
- Regime Brain v1 must mark the trend as eligible. Regime efficiency is used as a gate, not counted again as independent evidence because it overlaps Trend features.
- Structure Brain v1 can independently confirm or oppose the trend.
- Five-bar M1 momentum normalized by ATR14 can independently confirm or oppose the trend.
- Volatility Brain v1 vetoes the candidate on a completed-bar shock. In compressed volatility, a Structure breakout is required.

Candidate evidence is a deterministic weighted score: 60% Trend evidence, up to 25% same-direction Structure evidence, and up to 15% same-direction momentum evidence. A score is feature strength, not a win probability.

## Historical path

Select `meta_v2` in the native Backtest page. Its independent algorithm identifier
is `meta-v2-candidate-v1 / M1-OHLC-conservative-v1`. It consumes the existing
completed-bar Trend/Regime/Structure/Volatility assessment and requires
`Analysis.current` and candidate eligibility. The combined evidence gate is 0.55.

For a controlled signal comparison, it shares Meta v1's fill and risk model:
1.5 ATR stop rounded outward, target at 2R, 30-minute time limit, next-open entry,
prior completed-bar spread, slippage, commission, lot floor, daily and drawdown
limits. It does not apply candidate WAIT as an early exit. Gaps close an existing
position at the next observed open and block entry. The analysis freshness and
30-consecutive-bar requirements on each timeframe remain active.

Every tested minute stores `CandidateDecision`: time, preceding signal bar,
current-data status/reason, ATR, legacy action, the complete candidate (including
gates and evidence), and the simulator's admission outcome. This includes
position-occupied, loss-limit and data-gap minutes. Candidate features are known
before the minute opens; the outcome records the simulator's handling of that
minute, including exits. A candidate can be eligible while execution admission
is blocked. No candidate audit field is added to legacy strategy report JSON.

Choose a saved Meta v1 result, select Meta v2, and press the same-data experiment
button. The core freezes the original History and all options except strategy,
persists parent linkage atomically, and verifies complete history/options equality
before displaying the comparison. Costs cannot silently follow edited settings.
The report shows both strategies' net, trades, return, drawdown, profit factor,
win rate, commission and gap exits. Offline replay retains the same parent.
Exports include `comparison.json`, `decisions.csv` and report JSON/HTML.

Historical behavior, including changes to any shared brain dependency, requires
a new algorithm version if it would change a frozen report. Keep legacy IDs and
old report serialization stable; preserve golden fixture checks.

## Evaluation boundary

Synthetic tests check causal prefixes, costs, gates, missing/stale data, gap
handling, risk limits, old-report digests, persistence and deterministic replay.
The steady synthetic trend may produce identical Meta v1/v2 trades: this is not
an effectiveness result. Real broker datasets and separate out-of-sample periods
are still needed. Keep legacy Meta as the Shadow decision source until that
evaluation is reviewed. No broker order authority or profitability claim exists.
