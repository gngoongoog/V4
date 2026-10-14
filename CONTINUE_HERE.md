# Continue Here

GitHub is the canonical memory for BrainTrade OS. Resume from repository state, not conversational memory or old ZIP files.

## Read before editing

1. `BUILD_HANDOFF.md` — exact tested code checkpoint, validation and next action.
2. `PROJECT_STATE.md` — implemented vs pending state.
3. `ROADMAP.md` — ordered future work.
4. `docs/QUALITY_AUDIT.md` — repository-wide quality findings and open limitations.
5. `CHANGELOG.md` — completed milestones.
6. `docs/CONTINUITY_PROTOCOL.md` — checkpoint/resume rules.
7. Recent commits on `development` and the comparison with `main`.

## Current checkpoint

The active branch contains the 0.4.0 research milestone while Cargo/Windows preview metadata remain 0.3.0 until an explicit release bump.

Implemented research layers include Meta v1/v2, CES, KTE, offline trade review, Trend/Regime/Structure/Volatility, Gann, broker DOM Depth, Macro/News context, persisted chronological evaluation, Learning Foundation snapshots/labels/export/trainer/registry, manual review eligibility, read-only challenger inference and exact matured-label prediction scoring.

Legacy Meta remains the live Shadow decision source. Learned outputs are audit telemetry only. There is no champion activation and no broker-order route.

The repository audit also hardened SQLite foreign-key integrity, source/package hygiene, generated-file guards and Windows CI coverage on `development`.

## Exact next action

Follow `BUILD_HANDOFF.md`. The next scientific layer is a manual challenger performance-review policy over sufficiently large scored out-of-sample samples. It must remain read-only and must not auto-promote a champion or influence Shadow/broker execution.

## Non-negotiable continuation rule

- Commit coherent checkpoints to `development` and validate the changed area.
- Promote to `main` only after a deliberate safe-checkpoint review.
- Never claim a test passed unless it ran on the referenced code checkpoint.
- Never commit credentials, tokens, private runtime DB/logs, generated dependency trees, `.import`, encoded chunks or ZIP-only source.
- If documentation follows a tested code commit, clearly separate the documentation head from the executable validation boundary.
