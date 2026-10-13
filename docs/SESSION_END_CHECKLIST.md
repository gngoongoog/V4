# Session End Checklist

Before ending a development session or when limits are approaching:

1. Commit the latest coherent code to `development`.
2. Run every test available for the changed area.
3. Record test status as PASS / FAIL / NOT RUN / BLOCKED.
4. Update `BUILD_HANDOFF.md` with changed files, known issues and the exact next action.
5. Update `PROJECT_STATE.md` if feature/validation status changed.
6. Update `CHANGELOG.md` for completed milestones.
7. Promote only a safe/resumable checkpoint to `main`.
8. Confirm no credentials, tokens or private runtime data were committed.

If limits end unexpectedly, the next session must first inspect `development`, `main`, `BUILD_HANDOFF.md` and recent commits before editing anything.
