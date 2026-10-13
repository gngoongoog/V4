# 0.4.0 development checkpoint

This repository contains an **in-progress 0.4.0 source checkpoint** on `development`. KTE signal logic, stateful historical partial exits, deterministic regression coverage, and detailed ledger reporting/export are present. The current Linux CI validation is green; promotion to `main` waits for the current Windows CI packaging job to complete successfully.

No new end-user Windows release is declared by this checkpoint. The last delivered Windows portable remains 0.2.0 Preview. The CI package is a validation artifact, not a claim that native broker/MT5 acceptance has been completed on the user's laptop.

## 0.4.0 KTE validation completed on development

KTE historical execution now has deterministic checks for:

- broker-step 40/30/remainder partial sizing and rejection when three valid legs cannot be formed;
- TP1 partial close and next-minute break-even activation;
- TP2 partial close and next-minute ATR trailing activation;
- TP3 final close and aggregate trade accounting;
- conservative stop-first handling when stop and an unfilled target are both reachable in the same OHLC range, with ambiguity recorded;
- 45-minute time limit before TP1 and 90-minute limit after TP1;
- exactly three following bars of cooldown after a complete exit.

The saved `Trade` ledger is also the source for the desktop report, HTML report, chart fill markers and detailed exports. In addition to the existing summary `trades.csv`, exports now contain `fills.csv`, `stop_updates.csv` and `targets.csv`. The real-core integration harness verifies these files exist and that an unchanged offline replay reproduces the detailed ledger exports byte-for-byte.

Fresh Linux CI on the current code path passed **25 domain Rust tests**, **14 runtime Rust tests** with one explicit live-network test ignored, **16 Python tests**, the runtime core build and the real-core smoke harness. These tests use synthetic fixtures and establish software behavior only; they do not establish trading profitability.

## Still not validated as native live integration

Native Windows/MT5 historical retrieval against the user's actual broker/account, live Telegram delivery and any future broker execution remain separate acceptance steps. Broker order submission is still disabled/not implemented. Trained ML/ONNX, Gann, true order-flow/DOM, volume profile, news-informed trade decisions, walk-forward optimization, portfolio management and full restore remain roadmap items unless later commits explicitly change that status.

---

## 0.3.0 validated baseline

The earlier 0.3.0 source baseline provided the historical Meta/CES lab, asynchronous UTC daily MT5 history collection, strict account isolation, cancellation, saved reports/raw inputs, deterministic offline replay, same-data experiments, HTML equity curve and CSV/JSON exports. Official Fed/ECB RSS and private read-only Telegram `/news` and `/backtest` handlers were also present.

The earlier recorded baseline passed 18 domain + 13 runtime Rust tests, 16 Python tests, core smoke checks and Linux desktop render/build checks. That historical baseline remains documented for comparison; the fresh 0.4.0 development counts above supersede it for the KTE source checkpoint.
