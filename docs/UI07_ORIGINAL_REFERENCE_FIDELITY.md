# UI-07 — Match the original October 22 BRAINTRADE institutional mockup

## Reference recovered
The original user-approved reference is the dark trading-terminal design created October 22, with a left navigation rail, broker and account top strip, six dense KPI tiles, large XAUUSD candle/volume chart, a colorful order console, brain list, live positions, recent trades and a lower row of account/risk/latency panels. A light and smoke variation also existed. The actual source picture was recovered from the user's own prior Library assets and reviewed rather than guessed.

## Implemented natively (Rust + Slint)
- Slim rail with visually selected active page and 12 real page routing handlers; unchanged pages and state persistence.
- Six compact colored KPI tiles with actual Equity, Daily Net, Balance, Free Margin, Open Positions, and Quote Age: no fabricated drawdown, win rate or equity curve.
- Detailed chart from the MT5 source with the previously implemented tick-volume strip and controls.
- Color-coded START/STOP/SHADOW and account/mode navigation; START is disabled when the actual MT5 feed is offline. None of the navigation shortcuts overrides Core execution confirmation or account binding.
- Three independently grounded lower panels: current MT5 positions, stored broker trade-journal entries, and current Core brain monitor; followed by Equity/Net, risk monitor and broker status.
- Custom rectangle+TouchArea components are real buttons with their own callbacks. Existing 22-page application and all backend routes remain intact.
- All text reporting simulated evaluation is explicitly labelled. Unavailable data uses blank/waiting status rather than invented win rates or system-learning progress.

## Remaining fidelity work
The old reference image contains illustrative sample numbers, performance sparks, donut charts and 76% win rate. Those must not be copied as real measurements: implement them only after wiring and validating genuine time-series and closed trade outcomes. The reference also includes a decorative assistant illustration, which does not imply a working assistant service. Do not add dead-looking fake controls.

## Acceptance
GitHub Research/Windows CI render all native pages/themes and test input/button contracts. Visual review remains essential. User's connected MT5 Demo may be tested later on their Windows PC; it is never simulated as a real connected account in released production.
