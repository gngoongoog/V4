# Stage 3F-H — Conservative entry-time contract feasibility for Expert paired Paper

## Why a profitable Paper leg can never be treated as an executable broker trade

The prospective paired-coordinator trial uses a **fixed 0.01-lot** hypothetical position. Some MT5 brokers require **0.10 lot**, a larger margin, different volume steps, or impose stop and market-execution checks. A paper trade might show profit even when an MT5 order would be rejected. This adds **conservative descriptive entry-time evidence**; it does not modify either policy, trading gate, or position size.

At the moment the existing Stage 3F-G paper pair opens, store the observed:
- minimum, maximum, and step of broker symbol volume versus hypothetical 0.01 lot;
- free account margin and a **linear approximation** from the broker minimum-lot margin;
- MT5 trade and expert-trade permission fields;
- **separate** approximate entry-to-stop losses for frozen and adaptive legs, including configured roundtrip commission and two-sided slippage, compared with the current configured % risk budget.

Each field is `pass`, `fail`, or `unknown`. Any `fail` makes the combined pair `fail`; otherwise any missing/invalid broker evidence makes it `unknown`, **not pass**. Results remain in an immutable entry-time paper payload; changes to broker settings later never rewrite history.

The coordinator Brain Control Room now displays complete pairs split into `pass/fail/unknown`. Only the **indicatively passing** complete pairs enter the separate filtered mean Paper net-per-lot; the full unfiltered summary remains visible as before. Never relabel this analysis as realized PnL.

## Boundaries and validation

This is **not** `order_check`, `order_calc_margin`, `order_calc_profit`, exact symbol contract size, or an independent full broker risk-engine copy. Margin can be nonlinear; SL/TP distances may violate broker stop/freeze levels; live spread, price conversion, pending fills, slippage, portfolio exposure, risk switches, cooldowns, position ownership and account-wide loss gates can still deny entry. A nominal `pass` only means *these five heuristic fields did not detect a blocker*. It is not MT5 trade approval.

Unit tests cover: volume minimum greater than fixed Paper lot -> fail; missing broker contract data -> unknown; differing stop distances can make one policy pass and the other fail without any broker order. Run exact-head Linux/Research/Windows CI, then real Windows Demo evidence collection before approval.

**Next institutional increment:** independent stateful multi-symbol broker admission/order/fill/reconciliation simulation backed by forward MT5 demo observations, not a claim made by this partial contract probe.
