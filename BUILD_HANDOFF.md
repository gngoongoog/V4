# Current source: BrainTrade 0.8.0 — Demo/Sniper validation in progress

The user requested resumed building and execution verification on 16 October 2026; this supersedes the previous no-build instruction. Work remains on `feature/fast-execution-ui`. Native broker acceptance is still separate from simulated integration.

This continuation adds account/symbol commission profiles (round-trip or per-side), cost-correct Sniper levels refreshed at send, broker minimum-stop adjustment, configurable Sniper holding time, and execution diagnostics. It fixes research-desk desktop compilation and preserves commission profiles when general settings are saved.

Local evidence before CI: 64 domain + 52 runtime Rust tests passed (one public-network test ignored), 80 Python tests passed, and actual compiled Core + simulated MT5 open/close/reconciliation integration passed. Windows packaging and full desktop verification are pending. Current source has 21 pages and 15 execution identities.

The following is archived 0.7 delivery history. Its tests/artifact do not certify 0.8.

---

# BrainTrade OS — verified 0.7.0 Development Preview

Canonical repository: `gngoongoog/V4`. Verified executable/source checkout: `ba399ad206b1fe3231a10dbbd270de75145fd2e4` (tree `74cf92a0681b556c0b45462b511b11b2f7f5c62a`). This is GitHub’s PR test merge of head `e0547cab6ccce8d41944750f88d0089dc689ff29` into `58631430ccb823b63b22c344989a7c0e9ba93855`; its complete tree is identical to that PR head.

Research validation: PASS — https://github.com/gngoongoog/V4/actions/runs/38817498736

Windows build/package and Linux integration: PASS — https://github.com/gngoongoog/V4/actions/runs/38817498472

The exact source passed 61 domain Rust tests, 47 runtime Rust tests and 60 Python tests. One explicit public-network news test is intentionally ignored. Core, backup/restore, Studio, offline reports and simulated MT5 execution integrations passed. All 18 native pages were rendered at two sizes on both platforms. Portable Windows startup, DPAPI IPC, embedded Python/MetaTrader5 imports and PE dependencies passed.

The package includes 14 execution identities; Scalping, Swing, Expert and live-quote Sniper; actual coordinator/advisor/SOLO/RETURNING membership; proportional commissions; Wyckoff; 21-timeframe strategy generation; durable execution reconciliation; and the native dashboard. Expert adaptation uses prospective statistical outcomes, not an automatically promoted imported ML model.

Portfolio Exposure v1 is included and OFF by default. Its stopped-only editor is on the Execution page. When enabled it caps aggregate BrainTrade entry slots, lots and estimated stop risk; it never blocks exits or reconciliation. Optional per-brain risk budgets are also OFF by default. Required account binding, broker permissions, valid volumes/quotes and duplicate prevention remain enforced.

## Evidence and delivery

`docs/validation-v0.7.0.json` records this 0.7 checkpoint and artifact hashes. The 0.6 promoted history in `docs/validation.json`, `docs/validation-v0.6.0.json` and `docs/RELEASE_STATUS.md` stays unchanged. Later delivery documentation must not be presented as a newly compiled executable. The delivery ZIP records the executable source and the documentation commit separately and includes the complete source tree.

## Next steps

1. On the target Windows machine, follow `docs/QUICKSTART_AR.md` and perform native MT5 demo acceptance using `docs/ACCEPTANCE.md`.
2. Test private Telegram reports with credentials stored only on that machine.
3. Review/promote the 0.7 development branch through the normal PR path; never rewrite `main` or `development` history.
4. Continue genuine out-of-sample accumulation, ablation, aligned multi-symbol correlation research and historical-event replay with first-observed timestamps.

Startup/restart is STOPPED. SOLO never starts global execution automatically. STOP blocks new entries while owned timed exits continue with Core active; Full Exit stops client-side management, while broker-attached SL/TP remain with the broker. Native MT5 demo acceptance and live Telegram delivery have not been performed in CI. Imported ML challengers remain research-only, with no automatic Champion promotion. CI does not establish profitability, guaranteed latency or broker execution quality.
