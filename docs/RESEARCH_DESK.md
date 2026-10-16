# Research desk — 0.8 source continuation

Three research pages are added to the existing five-section navigation: correlation (18), observed news replay (19), and prospective member ablation (20). The dashboard now has 21 pages. They do not route broker orders or change strategy membership automatically.

## Correlation and per-symbol exposure

The primary configured symbol is included automatically, plus up to five exact broker symbol names. M1/M5/M15/H1 are selectable. A background reader requests 241 completed candles per symbol about once a minute and derives a fixed 240-period UTC grid of log returns. Both adjacent closes must exist; gaps are never filled or bridged. Every pair has an availability mask, overlap count and coverage. Pearson correlation is withheld below 60 aligned observations or for constant returns. Missing symbols are explicitly unavailable. Responses are rejected if account, symbol, configuration revision or freshness no longer matches.

Owned position lots are shown separately for each symbol. Net lots show buy-minus-sell direction; they are **not** a cross-asset money/risk conversion. No new correlation gate is applied to execution. This is the correlation research foundation from the roadmap, not a validated portfolio-trading rule.

Routes: `GET /v1/research-desk/correlation`, `POST /v1/research-desk/settings`, and gateway-only context request/report through the existing authenticated local API. Settings and the latest report per account/symbol/currency are persisted in SQLite.

## Observed news replay

Successful official-feed reads append headline revisions with the time Core actually received them. Re-fetching identical content creates no new revision. A changed title/topic/publication timestamp is a new revision; the older version is retained. Queries resolve the latest revision available at the requested cutoff, so a later correction does not rewrite the past. Source publication time is descriptive and never grants earlier availability.

`POST /v1/news/replay` takes an RFC3339 `at` timestamp (empty means now) and `hours` (1–720). It returns up to 200 known revisions and a truncation indicator. Coverage starts with actual observations after this update; no pre-install history is invented. This is an as-of information query for research. Event-return attribution and a news-conditioned backtest are still separate work.

## Prospective leave-one-member-out comparison

While Expert runs, one snapshot per completed M1 bar freezes advisor membership, weights, bid/ask, commission, contract values and slippage. It records the full vote and the vote excluding each member. SOLO members are absent. After 12 subsequent contiguous completed M1 bars, all variants are scored against the same exit bar and cost assumptions. Missing forward bars expire the observation. Startup/restart preserves samples in SQLite.

The report compares each exclusion with the full vote on exactly the same eligible samples. Positive `delta_without_minus_full` means the exclusion did better in those samples. Results are hypothetical per-lot directional diagnostics: overlapping windows, no stop/target execution, no swaps, no statistical independence and no automatic member selection. They are not broker PnL or a forecast. The report shows the most recent 5000 scored samples, plus pending/expired counts.

All three stores are included in the full SQLite backup. The archive format remains schema 12; new tables do not change its file format. The manifest explicitly advertises the new data.

## Verification

76 Python tests passed, including aligned/missing data, account-switch rejection, owned exposure and the actual SQLite news replay query. 54 Rust source files parsed without syntax errors. Slint 1.16.0b1 loaded the 21-page UI. Rust tests for revisions and ablation voting/costs were added but **not run**. No Cargo build/test, native desktop render, Windows package, MT5 broker acceptance or Telegram delivery was performed. Those remain distinct gates; prior 0.7 binary results do not certify this source.
