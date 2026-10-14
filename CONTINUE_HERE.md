# Continue Here

This repository is the canonical memory for BrainTrade OS. If a chat ends, a model changes, or a session is lost, resume from GitHub — never from conversational memory alone.

## Read in this order before any new code

1. `BUILD_HANDOFF.md` — exact last safe checkpoint, active task, files touched, tests and next action.
2. `PROJECT_STATE.md` — current product state and validation boundary.
3. `ROADMAP.md` — ordered next work.
4. `CHANGELOG.md` — completed checkpoints.
5. `docs/CONTINUITY_PROTOCOL.md` — rules that prevent work loss.
6. Latest Git commit(s).

Current checkpoint: **0.4.0 research in progress**. KTE, observational brains and
the offline trade-review chart, selectable historical Meta v2, matched-data
comparisons and candidate audit are in the current source; consult BUILD_HANDOFF.md
for exact tested commits. The 0.3.0-only state in old ZIPs is stale.
Windows packaging is deferred; the last delivered Windows portable remains 0.2.0 Preview.

Current validation note (2026-10-14): the first full chronological-evaluation run exposed three Rust borrow-lifetime errors in the Learning Foundation SQLite iterator blocks. Commit `48b1fb0f` fixes those iterator lifetimes without changing learning policy or data semantics. The next full research CI run must validate that fix together with chronological holdout/rolling boundary and future-causality tests before runtime evaluation persistence is added.

## Non-negotiable continuation rule

- GitHub is the source of truth.
- Work must be committed in small logical checkpoints; do not wait for an entire phase to finish.
- `development` stores ongoing/WIP checkpoints.
- `main` stores the latest safe checkpoint that is ready to resume from.
- After each meaningful code checkpoint, update `BUILD_HANDOFF.md` with the exact next action and test state.
- Never claim a test passed unless it actually ran on that checkpoint.
- Never commit passwords, broker credentials, Telegram tokens, API keys, `.env` secrets, private runtime databases or logs.

If interrupted mid-task, preserve the latest coherent work on `development` and record the unfinished state in `BUILD_HANDOFF.md` before doing anything else in the next session.