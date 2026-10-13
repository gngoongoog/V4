# Meta Brain v2 — candidate checkpoint

Meta Brain v2 is an observational candidate. It is visible in the existing brain analysis output but does not replace `Analysis.action`, does not change Shadow entries, and has no broker execution authority.

Inputs and gates:

- Trend Brain v1 supplies aligned direction and normalized trend evidence.
- Regime Brain v1 must mark the trend as eligible. Regime efficiency is used as a gate, not counted again as independent evidence because it overlaps Trend features.
- Structure Brain v1 can independently confirm or oppose the trend.
- Five-bar M1 momentum normalized by ATR14 can independently confirm or oppose the trend.
- Volatility Brain v1 vetoes the candidate on a completed-bar shock. In compressed volatility, a Structure breakout is required.

Candidate evidence is a deterministic weighted score: 60% Trend evidence, up to 25% same-direction Structure evidence, and up to 15% same-direction momentum evidence. A score is feature strength, not a win probability.

Promotion rule: keep legacy Meta as the Shadow decision source until Meta v2 receives a selectable historical backtest path and is compared on identical saved datasets with cost-aware metrics and no lookahead.
