# Repository comparison — 2026-10-14

Compared actual Git trees and changed source files, not repository names/version labels.

| Repository / branch | Checked head | Finding |
| --- | --- | --- |
| gngoongoog/BrainTrade-OS main | 41932be8715dd300c22329b4fb553d07db2ab521 | Earlier research baseline; documentation still describes KTE as in progress. |
| gngoongoog/BrainTrade-OS development | c1343a81b5d4fa050ee94ca52a2e7261b4952db7 | KTE ledger/export hardening; 58 tracked files. |
| gngoongoog/V4 development | f3bc12fb76ae3fa69a95971827d1f3f875ffd2b9 | 87 tracked files before this release work; contains every path from the other tree and 29 additional files. |

V4 is the more complete foundation. Its changed files preserve the KTE ledger,
export and Windows fixes while adding Meta v2, saved same-data experiments,
trade-review charts, structural opportunities, Gann/DOM, macro/news, chronological
evaluation and the learning registry/review/inference/scoring pipeline. Large
runtime/lib.rs differences include moving services into dedicated modules.

No unique source path was found in the other development tree. All 21 changed
implementation/test/workflow files were retrieved for comparison. The other
repository is preserved; this delivery continues V4/development without rewriting
BrainTrade-OS or promoting the older V4/main branch automatically.

The 0.4.0 release candidate adds missing dashboard surfaces, immediate STOP/stale
withdrawal, corrected-history cache invalidation, opportunity history/export and
native Windows portable startup verification. Exact release validation is recorded
in BUILD_HANDOFF.md and docs/validation.json after CI completes.
