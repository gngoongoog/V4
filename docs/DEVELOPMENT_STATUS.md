# BrainTrade OS 0.8.0 — source-only preview

This update was requested without a project build. No new Windows executable or package exists for it.

Implemented source: persistent local IPC, combined quote/command pulse, separate read-only MT5 worker, deadline-based polling (20 ms target for active Sniper), exit priority, optional precheck bypass, policy reuse, one-click Demo setup, expansion brain and five-section dashboard. The continuation adds correlation, observed news replay and member-ablation research pages (21 pages total). See `RESEARCH_DESK.md` for scope and `PROJECT_COMPLETION_AR.md` for remaining work.

Source verification is recorded in `development-validation.json`. Rust source syntax and interpreted Slint checks are not a substitute for Rust type-checking, desktop linking, packaged Windows checks or native MT5 acceptance. The 0.7 binary/evidence is unchanged and archived in `validation-v0.7.0.json`. Promoted 0.6 history is unchanged.

Next target-machine validation, when a build is requested: native dashboard at both supported sizes; Demo account start/stop; reconnect and account switch; journal recovery; measured gateway cycle and broker send latency. No real broker order or Telegram delivery was tested in this update.
