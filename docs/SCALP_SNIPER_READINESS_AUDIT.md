# Scalp / Sniper data-readiness audit — 2026-10-19

## Defect

Full research analysis requires fresh, completed **M1/M5/M15** bars. However, the existing Scalping and Sniper decisions use **M1/M5** with live executable quotes and do not use M15 for their trigger or alignment. The selection path returned early whenever full research was not current, so an otherwise valid M1/M5 entry could be vetoed solely by missing M15 data.

## Fix scope

- The full Expert/research path continues to require the existing three-frame evidence.
- Scalping and Sniper can proceed when M1/M5 independently pass strict checks, even if the separate M15 requirement has not matured.
- The fallback rejects missing or invalid market data, reported broker-read errors, snapshots older than 15 seconds, fewer than 100 completed M1/M5 bars, any non-finite/invalid candle, non-increasing times, incomplete last candle, stale last completed candle and any gap across the last 30 bars.
- Signal direction, actual breakout/momentum trigger, quote freshness, broker spreads/cost checks, account identity, discretionary START/STOP, portfolio constraints and duplicate-send journal remain unchanged.
- A new signal is never generated just because a missing M15 series is ignored. M1/M5 must still agree and produce an actual trigger.

## Tests

Added two deterministic Rust regression tests: missing M15 with valid M1/M5 permits *evaluating* Scalping/Sniper without fabricating BUY/SELL, and stale/incomplete/gapped M1/M5 still veto any entry. Research Expert retains its requirement.

## Limitations

If the MT5 gateway explicitly reports a feed error, the new path still waits, even if its existing M1/M5 buffers happen to look complete. Real trading frequency, win rate, market data quality, and MT5 Demo execution remain unverified until tested on the intended Windows PC. This is a **data dependency bug fix**, not evidence of profitable trading.
