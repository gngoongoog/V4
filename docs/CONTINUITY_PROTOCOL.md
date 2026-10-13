# BrainTrade OS — Continuity Protocol

Purpose: prevent loss of work when a ChatGPT conversation, model session, device session or usage window ends unexpectedly.

## Branch model

- `main`: latest safe/resumable checkpoint.
- `development`: ongoing work and frequent WIP checkpoints.

Do not keep important work only in chat text or a temporary local workspace.

## Checkpoint cadence

Commit after each coherent batch, for example:

- one completed function/module change;
- one migration/refactor batch;
- one test batch;
- one UI page/feature integration;
- one bug fix with its verification.

Do not wait until a large multi-hour phase is finished before pushing.

## Required checkpoint metadata

At every meaningful checkpoint update `BUILD_HANDOFF.md` with:

- active task;
- last safe point;
- files/modules changed;
- tests actually run and their result;
- known failures or untested assumptions;
- exact next action.

Update `PROJECT_STATE.md` when feature status or validation boundaries change. Update `CHANGELOG.md` when a user-visible or architecture milestone is completed.

## Resume algorithm for any future session

1. Open `CONTINUE_HERE.md`.
2. Read `BUILD_HANDOFF.md`.
3. Compare `development` with `main` and inspect recent commits.
4. Prefer repository state over conversational recollection.
5. Re-run or inspect the relevant tests before making claims about correctness.
6. Continue from the exact next action recorded in the handoff.

## Interrupted work

If work is partially complete but coherent enough to preserve:

- commit it to `development` with a clear `WIP:` commit message;
- describe what is incomplete in `BUILD_HANDOFF.md`;
- do not promote it to `main` until it is safe/resumable.

If a change is not coherent enough to commit, the next session must first inspect the repository and recreate only the uncommitted delta from the explicit handoff notes. Avoid depending on hidden scratch state.

## Testing truthfulness

Allowed statuses:

- `PASS` — test actually ran and passed on the referenced commit;
- `FAIL` — test actually ran and failed;
- `NOT RUN` — not executed on that checkpoint;
- `BLOCKED` — cannot run in the current environment.

Never convert prior-version test results into fresh validation for a newer checkpoint.

## Secret handling

Never commit:

- `.env` secrets;
- Telegram bot tokens;
- API keys;
- broker/MT5 passwords or credentials;
- private keys;
- personal runtime databases/logs containing sensitive data.

Commit templates/example config only with fake placeholders.

## Recovery objective

A new session with no chat history should be able to reconstruct the complete project state and continue development using only:

- the GitHub repository;
- `CONTINUE_HERE.md`;
- `BUILD_HANDOFF.md`;
- `PROJECT_STATE.md`;
- `ROADMAP.md`;
- `CHANGELOG.md`;
- recent Git commits.
