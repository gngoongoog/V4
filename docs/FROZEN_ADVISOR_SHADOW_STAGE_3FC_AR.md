# Stage 3F-C — Frozen Advisor Shadow Trial (Demo only)

## Actual scope and why
This stage freezes an **individual non-system advisor BrainConfig and, for generated brains, selected Genome**. It also freezes the account Config (commission per lot, slippage, volume, symbol and trading mode) used in the quote-sampled paper trade. Each fixed clone runs on *future observed prices* beside the current adaptive advisor. It is **not** a freeze of the system-wide coordinator, Sniper learner, continuously updated MT5 brain weights, or a future outcome backtest.

## Controls
In "حالة العقول والتحكم", while STOPPED and bound to a fresh real MT5 **Demo**, choose a subordinate advisor under **Expert**. Click "جمّد نسخة تجريبية". One running trial per account/symbol/mode/advisor. The stored policy is immutable across application restarts. To create another version, first "إنهاء التجربة المجمّدة"; the original result remains archived, and the next trial gets a fresh unique ID.

The API `POST /v1/execution/frozen-shadow` checks STOPPED + no unsettled commands, exact account/symbol/context and selected mode, Demo trade_mode and source brain identity, and supports `freeze` or `stop`. `GET` shows the current bound account trial listing. The research model Challenger and REAL execution are untouched.

## Runtime
On each one-second sampled observation, calculate the frozen BrainConfig strategy with the frozen optional Genome and old Config, while displaying the actually observed adaptive signal. Only bars with `bar_time > freeze time` are compared, once per run and closed bar; no historical/backfilled scores. A separate durable `frozen_shadow_signal_pairs` table records both outputs at the same bar, no artificial fills.

Independent simulated quote trades use existing price-sampled `shadow_paper::observe` ledger under a synthetic `frozen_<UUID>` identity. It includes bid/ask and the fixed 2-sided slippage and full round-trip assumed commission; quote gaps may skip trigger levels, so results **are not actual broker fill results**. Even if stopped, a previously opened virtual position remains eligible for closing on a new quote, but no new frozen entries are opened.

The frozen trial never invokes order_send, has no valid MT5 magic/BrainTrade broker identity, and never updates the adaptive learning weights or decision coordinator. Its state does not share the same identity as regular Paper positions or the broker journal.

## Results and limitations
Per-brain panel shows exact frozen run ID and state, pairs of same-bar signals, both ready count, same-direction count, closed simulated trades, wins and paper net after assumed costs, with detailed provenance warning.

The *adaptive* side is observed at the same bar, but its regular Paper trades can have different timestamps, parameter changes, execution path and fill assumptions. This is descriptive parity monitoring, NOT a controlled claim of strategy alpha or institutional production-readiness. Genuine comparison of the entire evolving coordinator needs versioned policy snapshots, independent out-of-sample forward trials, predeclared metrics, and identical fills/cost models.

## Synthetic tests and release
Rust tests: immutable policy identity, account isolation, no duplicate active run, stop+re-freeze preserving history, reject system/Sniper/generated-without-Genome/wrong mode. Exact-head Research+Linux+Windows CI and UI rendering required. Real-money LIVE is unsupported as a frozen-trial target.
