# Current source: BrainTrade 0.8.0 — Demo/Sniper validation in progress

The user requested resumed building and execution verification on 16 October 2026; this supersedes the previous no-build instruction. Work remains on `feature/fast-execution-ui`. Native broker acceptance is still separate from simulated integration.

This continuation adds account/symbol commission profiles (round-trip or per-side), cost-correct Sniper levels refreshed at send, broker minimum-stop adjustment, configurable Sniper holding time, and execution diagnostics. It fixes research-desk desktop compilation and preserves commission profiles when general settings are saved.

Local evidence before CI: 64 domain + 52 runtime Rust tests passed (one public-network test ignored), 80 Python tests passed, and actual compiled Core + simulated MT5 open/close/reconciliation integration passed. Windows packaging and full desktop verification are pending. Current source has 21 pages and 15 execution identities.

The following is archived 0.7 delivery history. Its tests/artifact do not certify 0.8.

---

# Continue from verified BrainTrade 0.7.0 Development Preview

The 0.7 test checkout `ba399ad206b1fe3231a10dbbd270de75145fd2e4` (identical tree to PR head `e0547cab6ccce8d41944750f88d0089dc689ff29`) passed Research `38817498736` and Windows/Linux `38817498472`. Read `BUILD_HANDOFF.md` and `docs/validation-v0.7.0.json`; do not restart the version bump or reuse old 0.6 artifact evidence.

Portfolio Exposure v1, four modes, SOLO/return, adaptive coordinator weights, execution recovery and the 18-page dashboard are implemented. The ZIP delivery updates documentation/evidence only; executable provenance remains the tested source above.

Next user step: native Windows + Demo MT5 acceptance with `docs/QUICKSTART_AR.md` and `docs/ACCEPTANCE.md`, then private Telegram delivery using local credentials. Neither has been performed by CI. Next repository step: review/promote the 0.7 PR normally, preserving 0.6 historical evidence and branch history. Scientific work still open is listed in `ROADMAP.md`; do not claim automatic imported ML promotion, profitability or HFT latency.
