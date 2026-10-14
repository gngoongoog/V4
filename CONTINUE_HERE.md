# Continue Here

GitHub `gngoongoog/V4` is the canonical memory for BrainTrade OS. Resume from repository state, not conversational memory or old ZIP files.

## Read before editing

1. `BUILD_HANDOFF.md` — exact validated executable checkpoint and next action.
2. `PROJECT_STATE.md` — implemented vs pending state.
3. `ROADMAP.md` — ordered future work.
4. `docs/validation.json` — machine-readable validation boundary.
5. `docs/QUALITY_AUDIT.md` and `CHANGELOG.md` — audit/history.
6. Recent commits and current CI on `development`.

## Current checkpoint — 2026-10-14

Release candidate: **0.4.0 Preview**.

Validated executable checkpoint: `74e6387e3f66b8035f5ce55db92561ec1809ebf4`.

- Research source validation `38586058214`: **PASS**.
- Windows build/tests `38586058215`: **PASS**.
- Windows artifact: `BrainTrade-Windows-x64-v0.4.0-Preview`, 33,229,148 bytes, SHA-256 `e9e09749359a8bfd4cc14959d141d1c8485bf3fe427887413f4b2c5274db0237`.

The Challenger Performance Review Policy is now implemented as a separate read-only audit layer over immutable scored challenger outcomes. It adds fixed sample/class/context/window/regime gates, transparent baselines, calibration/stability/drift checks, immutable review evidence, reason codes, APIs and Learning-page visibility. It cannot create a Champion and cannot influence Legacy Meta, Shadow, risk or broker execution.

Backup manifest schema is now 9 and includes immutable challenger performance-review records. Full restore remains unimplemented.

## Exact next action

The next scientific work is **real out-of-sample evidence accumulation and native acceptance**, not another automatic model/promotion layer:

- collect enough real broker completed-bar observations and matured labels for the fixed review policy to become meaningful;
- run native user-laptop MT5/broker + Telegram acceptance when that environment is available;
- keep Champion activation, automatic promotion and broker execution disabled unless separately designed and validated later.

## Non-negotiable continuation rule

- Commit coherent checkpoints to `development` and validate the changed area.
- Promote to `main` only after deliberate safe-checkpoint review.
- Never claim a test passed unless it ran on the referenced executable checkpoint.
- If documentation follows a tested code commit, clearly separate documentation head from executable validation boundary.
- Never commit credentials, tokens, private runtime DB/logs, generated dependency trees, `.import`, encoded chunks or ZIP-only source.
