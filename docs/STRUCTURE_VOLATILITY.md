# Structure + Volatility Brain v1

Checkpoint scope: observational research only. No broker execution and no Meta decision change.

- Structure Brain evaluates completed M1/M5/M15 candles against each timeframe's prior 20-bar range. It reports directional breakout consensus only when at least two timeframes break in the same direction and none break opposite.
- Volatility Brain compares M1 ATR14 with an earlier completed-bar true-range baseline and separately reports a last-bar shock when the latest true range is at least 2.5x ATR.
- Both outputs are nested under Regime Brain for backward-compatible status serialization and are visible through the existing `/brains` text path.
- Existing Regime label thresholds and the legacy Meta/Shadow entry decision remain unchanged in this checkpoint.

These evidence values are normalized feature strength, not calibrated win probabilities.
