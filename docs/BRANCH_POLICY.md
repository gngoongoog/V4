# Branch Policy

- `main`: latest deliberately promoted safe/resumable checkpoint.
- `development`: active research and coherent WIP checkpoints.

Current GitHub branch protection is **not enabled** on either branch. Until repository administration enables required checks, this policy is procedural rather than server-enforced.

## Development rule

Commit coherent changes to `development` and run/inspect validation appropriate to the changed area. Keep `BUILD_HANDOFF.md`, `PROJECT_STATE.md`, `ROADMAP.md` and validation records synchronized with real code state.

## Promotion rule

Do not fast-forward/merge `development` to `main` merely because one test is green. A promotion requires a deliberate checkpoint review with:

- relevant Research source validation green;
- Windows build/tests green when Windows/package-sensitive code changed;
- no known repository hygiene/security contract failure;
- exact executable validation boundary recorded;
- documentation/continuation state reconciled;
- no credentials/private runtime artifacts in the tree.

A documentation-only commit may follow a tested executable checkpoint; it must not be described as freshly validating executable behavior unless CI actually ran on that documentation head.

## Desired repository protection

When GitHub administration is available, enable branch protection/rulesets for `main` and preferably `development`, requiring the Research and Windows workflows (or their stable named checks) before merge/promotion.

If a session is interrupted, inspect `development` before making new changes because it may contain newer preserved work than `main`.
