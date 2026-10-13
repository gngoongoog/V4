# Volume Profile Brain v1

This checkpoint is observational research only. It does not change legacy Meta, Meta v2 candidate eligibility, Shadow entries, or broker execution.

## Data boundary

The brain uses only **completed M1 candles** and the `tick_volume` supplied by the connected MT5 broker. This is explicitly labeled `broker_tick_volume_proxy`.

It is **not**:

- exchange traded volume;
- the worldwide gold order book;
- broker DOM / market depth;
- executed buy/sell order-flow delta.

If tick volume is zero, the brain abstains instead of inventing volume.

## v1 calculation

- Lookback: last 120 completed M1 bars (minimum 60).
- Price bins: 24 equal-width bins across the observed high/low range.
- Each candle's tick count is assigned to the bin containing HLC3 typical price. No intrabar volume distribution is fabricated.
- POC: center of the bin with the largest tick count.
- Value Area: approximately 70% of tick volume, expanded outward from POC using the larger adjacent bin first.
- Relation: last completed close is `ABOVE_VALUE`, `BELOW_VALUE`, or `INSIDE_VALUE`.
- Additional context: POC concentration and normalized entropy of populated bins.

The directional context is descriptive only and is not an entry signal or win probability.

## Promotion gate

Before Volume Profile can influence Meta, it needs historical ablation on identical datasets and costs, plus evidence that the broker's tick-volume proxy adds information beyond Trend/Regime/Structure/Volatility. DOM/order-flow features remain unavailable until a real broker/feed source exposes those fields; absence must remain an abstention rather than synthetic data.
