# BRAINTRADE V4 — Enterprise V25 RC2 (Windows / MetaTrader 5)

> **Current source line:** Enterprise V25, version `0.25.1`, release channel `release_candidate`. The portable desktop entry point is **`BrainTradeNext.exe`** (React + Tauri + Rust Core + embedded Python Gateway); the historical `BrainTrade.exe` and 0.7.x Slint instructions are archived, not the current portable entry point.  
> **Trading status:** Rust Core and Gateway support operator-selected **Demo or Live account binding and regular trade execution**, but the source is **not independently LIVE-broker-certified**, not proven profitable, and not guaranteed low-latency. Passing CI does not prove a native broker order was executed on the user's account. The specialized automatic TP1/TP2/TP3 staged-close feature remains **intentionally disabled at final authorization** until full order/Deal reconciliation is implemented and tested.

**عربي:** BRAINTRADE V4 Enterprise V25 يتضمن أربعة أوضاع: **Sniper وScalping وSwing وExpert**. يبدأ بحالة **STOP**. اختيار **Demo أو Live بيد المستخدم** عبر أزرار الواجهة ويربط الحساب الحالي في MT5 فقط بعد مطابقة الهوية؛ الاختيار وحده ما يفتح صفقات. التداول العادي يدعم النوعين، لكن **TP1/TP2/TP3 الجزئي التلقائي ما زال مقفلاً**؛ الحجز والمعاينة ليست تنفيذ. المخاطرة الأساسية 2% مع إذن Live مستقل لرفعها حتى 5%، وخيار إدارة المخاطر منفصل؛ تأكد من المارجن والسبريد والعمولة والانزلاق. **لا يوجد اختبار موثق يثبت نجاح التداول الحقيقي أو ربحيته.**

## Current architecture and operational boundaries

- **Execution:** Rust Core with durable broker intent/reconciliation, Python MT5 Gateway, and optional `BrainTradeSniperBridge.mq5` tick/wake transport through an authenticated local named pipe. The EA is **transport-only**; it does not independently call MT5 `order_send`.
- **Modes:** Sniper (tick path), Scalping, Swing, Expert. Mode and account identity are explicit. STOP prevents **new entries**, but does **not** necessarily liquidate owned positions; read the execution log and existing broker SL/TP.
- **Brains:** 17 durable identities, including `system`, Gann, Wyckoff, Trend, Momentum, Structure, Zones, broker DOM, candle-activity Profile, Gann+DOM+Profile, CES, KTE, Regime, Generated, Liquidity Sweep, Trend Pullback, and Sniper Learner. These are **heterogeneous deterministic rules/adaptive components**, not 17 independent neural models. Preserve their IDs/magic mappings for old orders.
- **SOLO / Expert:** SOLO can select an independent active entry owner **subject to Core broker gates**, STOP, exact account identity and reconciliation; changing modes or ownership is not a direct trade authorization. Expert may coordinate eligible advisors. Nonactive or research-only samples are not broker fills and do not silently promote to LIVE.
- **Learning:** Verified matched *closed MT5 Demo* outcomes can update designated adaptive selection weights. Other prospective completed-bar and inactive-mode Forward data are distinct research. No claim that Square-of-Nine, Wyckoff phases or candle profiles are proven forecasting laws.
- **Market data:** DOM is the **MT5 broker book**, not centralized COMEX; a Gateway *local receipt time* is required for recent reads, but does not certify upstream broker publication time. Profile/HVN-style candle analyses can be **tick-activity proxies**, not actual exchange volume at price. Gann Square9/fan/cycles added as **research-only observations**, not automatically executable gates.
- **Trading constraints:** Exact broker identity, market freshness, durable journal, duplicate/uncertain-send reconciliation and owner authorization remain mandatory. Fees, stop distances, spreads and tick values must be checked against the actual broker. All trading may lose money, especially on small accounts and 1:500 leverage.

## Windows portable: operator-selected Demo / Live binding (Demo acceptance first)

1. Download the **portable Windows x64** artifact built for the intended **exact Git commit** in GitHub Actions (workflow `V25 Enterprise Windows Portable Release Candidate`), and verify its manifest/built-in `VERIFY_SYSTEM.cmd`. **A file from an earlier run is not the latest build**.
2. Extract the **entire ZIP** to a fresh folder. Do not run or copy the EXE alone; keep `runtime/`, `mt5/` and bundled support files. Windows 10/11 x64 and WebView2 are required; install standard MT5 Windows separately.
3. Run **`BrainTradeNext.exe`**, configure the real `terminal64.exe` path and **the precise broker symbol** (`XAUUSD`, `XAUUSDm`, etc). Confirm Demo identity and balance; attach/compile `BrainTradeSniperBridge` where necessary, checking that EA is sending ticks rather than only `WAITING`.
4. Check `STOP`, save/reload Sniper settings, receive timestamped ticks, place **one intentionally authorized Demo** test order if safe, inspect actual MT5 deal history/fees, and confirm Core reconciliation. Full Exit should terminate app-owned processes; verify Windows Task Manager and logs, not only a UI toast.
5. Preserve the G7-H Demo acceptance report, broker log and exact source/build SHA. Do **not** use REAL until independent risk review and on-device broker acceptance. Do not paste Telegram tokens, account secrets, or raw private order histories into public GitHub issues.

**Current staged-TP boundary:** the Rust TP contract, SQLite reservation and account-scoped Deal audit are testable but **do not queue an automatic broker partial-close**. Ordinary broker SL/TP and verified full-close processing remain independent. See [main-promotion readiness and open acceptance gates](docs/v25/MAIN_PROMOTION_CHECKLIST_20261103_AR.md).

**Operator manual:** [V25 Arabic Windows portable guide](docs/v25/USER_MANUAL_V25_AR.md) · [G7-H actual Demo acceptance](docs/v25/G7H_NATIVE_DEMO_ACCEPTANCE_AR.md) · [Arabic quick start (historical 0.7.x: read V25 manual first)](docs/QUICKSTART_AR.md).

## Research, strategy evaluation, and known limits

- The Core backtest API accepts a **nonempty, UTC minute-aligned period of at most 30 days**. Shorter-than-one-day windows are supported for research and regression tests; older date-only UI guidance may suggest whole-day selection.
- The backtester simulates completed **M1 Bid OHLC**, next-bar entries, historical spread proxy, user-set commission/slippage, deterministic stops and conservative stop-before-target resolution. It **cannot reproduce intra-minute tick ordering, actual network/MT5 fills, variable historic tick value, funding/swap, or hidden order flow**.
- CES v1 keeps its original tick-volume EPI behavior and report identity; `ces_range_research` uses the candle-range EPI **only for historical comparison**, not live execution.
- The saved-history page contains **«مقارنة معادلتي CES»** and **«مقارنة العقول مقابل Trend»**, comparing Wyckoff, Liquidity Sweep and Trend Pullback against a Trend control under **common 1.5 ATR / 2R / 30-minute simulated exits**. These compare **entry signals**; they intentionally do not clone each brain's broker-side structural exits.
- Historical holdout windows are chronological but **not prospectively frozen independent OOS**. Results with gaps or fewer than 30 closed hypothetical trades for any policy are labelled inadequate. Even larger windows are descriptive only. No historical experiment automatically changes Expert weights, activates Generated, or places MT5 orders.
- Heavy saved-history research is **blocked while START, broker execution, active Paper position, or another backtest is running**, because synchronous replay could delay Core servicing. Research-only Tauri requests have a bounded extended timeout; broker/trading timeouts remain separate.
- Eligible Generated Studio reports have a constrained rule search/review pipeline, not unlimited GA/AI strategy invention, and lab eligibility is a **minimum broker-entry gate**, not evidence of out-of-sample profitability.

**Detailed V25 source-backed reports:** [16-brain audit](docs/v25/BRAIN_BY_BRAIN_LOGIC_AUDIT_AR_20261102.md) · [brain repair gates](docs/v25/BRAIN_REPAIR_GATES_20261102_AR.md) · [Gann geometry and learning](docs/v25/GANN_V25_RESEARCH_LEARNING_AR.md) · [CES EPI research](docs/v25/CES_EPI_CHRONOLOGICAL_ABLATION_AR.md) · [Wyckoff/Liquidity/Pullback vs Trend research](docs/v25/BRAIN_SIGNAL_ABLATION_WYCKOFF_LIQUIDITY_PULLBACK_AR.md).

## Build and validation

- **Windows:** `./scripts/Build-V25-Portable-Windows.ps1` runs the Windows build/tests, relocation/package validation and native WebView tests in GitHub Actions. The workflow uploads a Windows portable ZIP and validation evidence **for its own commit SHA**; manual source archives are not portable runtime packages.
- **Core:** `cargo test -p braintrade-domain -p braintrade-runtime --locked`.
- **UI:** `npm --prefix apps/terminal ci && npm --prefix apps/terminal test`.
- **Gateway/tests:** `python -m unittest discover -s tests -v`.
- **Validation:** Windows/Native EA/pipe/full-exit/G7 CI results must be inspected on the **specific HEAD** to be promoted to `main`. The project may pass synthetic integration checks while actual broker Demo acceptance is still pending.

**Document precedence:** this README and V25 manuals describe the current architecture; older `docs/DEVELOPMENT_STATUS.md`, `docs/QUICKSTART_AR.md`, and `docs/EXECUTION_AND_ROLES.md` contain historical 0.7.x notes, not a replacement for V25 instructions. A published commit/release does **not** equal verified real-money trading readiness.
