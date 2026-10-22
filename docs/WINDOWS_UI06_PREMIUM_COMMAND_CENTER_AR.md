# UI-06 — Premium Command Center, native Windows Rust/Slint

## Objective
UI-05 was functionally connected but visually too basic: oversized KPI rows pushed the XAUUSD chart down and left little room for the decision console. This update restructures rather than simply recolors.

## Delivered
- Premium command banner and grouped left rail. Hero uses real Core online/state/execution-active data, no invented success score.
- Four main economic KPIs in one row plus compact live positions/quote-age strip, preserving real data.
- 304px OHLC chart with grid and 48px Tick Activity histogram using the real MT5 tick_volume of the last 70 completed bars.
- ChartCandle.volume is normalized safely in Rust. No candles or volume are invented on offline or stale accounts.
- Existing action handlers and pages stay in place; no changes to MT5 orders, risk settings, stored learning, or auto-execution.

## Acceptance
- Required: Research native render at 22 pages × three themes × two viewport sizes; Windows x64 packaged build; Core and UI contract tests.
- Still required: user reference screenshot for pixel matching, operator mouse/keyboard review, and Windows MT5 Demo acceptance.
- Do not claim 100% visual identity or live trading safety without those acceptance checks.