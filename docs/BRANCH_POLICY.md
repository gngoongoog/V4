# Branch Policy

- `main`: latest safe/resumable checkpoint.
- `development`: active development and WIP checkpoints.

Future coding sessions should commit frequently to `development`. Once a checkpoint is coherent and verified enough to resume safely, promote/fast-forward it to `main` and update `BUILD_HANDOFF.md`.

If a session is interrupted, inspect `development` before making any new changes. It may contain newer preserved work than `main`.
