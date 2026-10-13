# 0.4.0 development checkpoint

This repository currently contains an **in-progress 0.4.0 source checkpoint**.
KTE signal logic and a stateful historical partial-exit path are present, but final
KTE regression coverage, chart/export review and validation-artifact reconciliation
are still in progress. Do not treat KTE as a fully validated release feature yet.

The last validated source baseline remains 0.3.0. Windows packaging is still deferred
and the last delivered Windows portable remains 0.2.0 Preview.

---

## 0.3.0 validated baseline

Windows packaging is explicitly deferred by Mohammed. No new 0.3.0 EXE or Windows
portable is delivered. Last delivered portable remains 0.2.0. The user confirmed
v0.1.1 launch, MT5 connection and visible balance; later additions need laptop tests.

Implemented in this source: native historical lab, asynchronous UTC daily MT5
history collection, strict account isolation, cancellation, saved reports/raw
inputs, deterministic offline replay, same-data Meta/CES experiments, HTML equity
curve and complete CSV/JSON exports. CES follows the supplied Pine defaults with
explicit tick-volume and next-open-fill differences. See BACKTEST.md.

Official Fed/ECB RSS has a background worker, persistent cache, source-specific
freshness/errors, manual refresh and enable/disable controls. The News page opens
only official article URLs. Current headlines are context only, not a calendar,
sentiment model or trade gate. See NEWS.md. Telegram adds private read-only /news
and /backtest handlers; no live bot delivery has been tested.

Current reconstructed source passed 18 domain + 13 runtime Rust unit tests,
16 Python tests, and the real-core integration harness: auth, lifecycle, staleness,
shadow entry/exit and persistence, history chunks/cancel/account isolation,
exports, exact offline replay, CES experiment, news settings, backup and restart.
Live RSS parse and actual background worker passed: three official feeds, 44
unique cached headlines, responsive core and cache persistence after restart.
Linux release desktop compiled successfully. All nine pages rendered at
1120x820 and 880x700 (18 images); layouts were visually reviewed. The docs/ui-*.png
files show current 0.3.0 marked render fixtures. See validation.json. All test
trades use explicitly synthetic data.

No broker execution/protection, trained ML, KTE/Gann, true order-flow/DOM,
news-informed decisions, walk-forward optimization, portfolio manager or full
restore. These remain roadmap, not implemented capabilities. No profitability,
win-rate or latency claim. Native Windows, MT5 historical retrieval, bot delivery
and parity with TradingView are not validated here.
