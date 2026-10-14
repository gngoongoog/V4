# BrainTrade OS 0.4.0 Preview — release candidate

The user explicitly requested a current Windows portable. This candidate uses
V4/development, the fuller repository identified in REPOSITORY_COMPARISON.md.
Cargo, Cargo.lock, native UI and package version are now 0.4.0.

Includes the existing analytical/research/learning stack, plus dedicated Opportunities,
Market Map and Learning pages, persisted opportunity history/export, immediate STOP
withdrawal and corrected-history cache invalidation. The one launcher is BrainTrade.exe;
Core and the bundled Python MT5 bridge start automatically. MT5 itself must already
be installed and the account saved locally.

Candidate validation: pending fresh Linux/Windows CI. Do not mistake earlier 0.3.0
results for this build. Final provenance will be recorded after CI completes.

Broker execution, automatic champion promotion and full backup restore remain
unimplemented. The application analyzes and simulates; it does not send MT5 orders.
Native broker/history/DOM and private Telegram acceptance still require the user's
Windows laptop. No synthetic metric proves profitability.
