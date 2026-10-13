# BrainTrade OS — Build Handoff

This is the first file to read when resuming work after a chat/session interruption.

## Canonical repository

- Repository: `gngoongoog/BrainTrade-OS`
- Stable branch: `main`
- Ongoing work branch: `development`
- Current development checkpoint: **0.4.0 in progress**
- Last validated source baseline: **0.3.0**
- Last delivered Windows portable: **0.2.0 Preview**

## Last safe checkpoint

The source tree is stored as normal GitHub files and folders. The core project structure is present (`crates/`, `gateway/`, `scripts/`, `tests/`, `docs/`) with no `.import` staging/chunk mechanism in the working tree.

## Current active work

Finish the 0.4.0 KTE checkpoint before starting the later analysis brains.

Priority order:

1. Complete KTE unit/regression tests.
2. Validate partial exits, break-even, trailing and cooldown edge cases.
3. Verify KTE chart/report/export output uses the same saved trade ledger.
4. Reconcile `docs/validation.json`, release status and project state.
5. Run the full Rust + Python test suites in a Rust-capable environment.

## Known validation state

- Python gateway/packaging suite previously recorded: **16/16 passed** on 2026-10-13 in the ChatGPT build environment.
- Rust tests were not freshly rerun during the GitHub migration.
- The prior 0.3.0 checkpoint records 18 domain + 13 runtime Rust tests as passed; do not treat that as fresh 0.4.0 validation.

## Resume procedure

1. Read this file, `PROJECT_STATE.md`, `ROADMAP.md`, `CHANGELOG.md` and `docs/CONTINUITY_PROTOCOL.md`.
2. Inspect the latest commits on `development` first. If it is ahead of `main`, continue from `development`.
3. Before editing, write down the exact active task and expected acceptance test in this file.
4. Commit small coherent batches to `development` frequently.
5. After tests pass for a resumable checkpoint, update this file and fast-forward/promote the safe state to `main`.
6. If a session ends before completion, the latest coherent `development` commit is the recovery point.

## Rules for future ChatGPT sessions

- Do not rely on previous-chat memory when GitHub disagrees.
- Do not rebuild files from memory if the repository already contains them.
- Do not overwrite newer repository changes without comparing first.
- Do not mark unfinished features as complete.
- Do not commit credentials or secret local configuration.

## Next exact action

Continue **P0 / KTE validation** from `ROADMAP.md`. Inspect current KTE implementation and existing tests, add missing deterministic regression coverage, run available tests, then update this handoff with results and the next action.
