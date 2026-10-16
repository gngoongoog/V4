# Current source: BrainTrade 0.8.0 — not built

Continue from branch `feature/fast-execution-ui`, based on the delivered 0.7 preview. The user explicitly requested **no build**: do not run Cargo build/test, create a Windows package or trigger CI unless later requested.

Current changes are described in `docs/FAST_EXECUTION_UI.md`; source verification and exclusions are in `docs/development-validation.json`. Persistent IPC, read-only MT5 worker, fast pulse, Demo start, expansion brain (15 identities) and the five-section UI are implemented in source. No native MT5 order or latency acceptance has been performed. Rust type-checking and target Windows rendering remain pending.

The following is archived 0.7 delivery history. Its tests/artifact do not certify 0.8.

---

# Continue from verified BrainTrade 0.7.0 Development Preview

The 0.7 test checkout `ba399ad206b1fe3231a10dbbd270de75145fd2e4` (identical tree to PR head `e0547cab6ccce8d41944750f88d0089dc689ff29`) passed Research `38817498736` and Windows/Linux `38817498472`. Read `BUILD_HANDOFF.md` and `docs/validation-v0.7.0.json`; do not restart the version bump or reuse old 0.6 artifact evidence.

Portfolio Exposure v1, four modes, SOLO/return, adaptive coordinator weights, execution recovery and the 18-page dashboard are implemented. The ZIP delivery updates documentation/evidence only; executable provenance remains the tested source above.

Next user step: native Windows + Demo MT5 acceptance with `docs/QUICKSTART_AR.md` and `docs/ACCEPTANCE.md`, then private Telegram delivery using local credentials. Neither has been performed by CI. Next repository step: review/promote the 0.7 PR normally, preserving 0.6 historical evidence and branch history. Scientific work still open is listed in `ROADMAP.md`; do not claim automatic imported ML promotion, profitability or HFT latency.
