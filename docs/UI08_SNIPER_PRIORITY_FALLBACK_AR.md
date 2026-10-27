# UI-08 — Sniper Primary / Secondary Coordinator Handoff

## User intent
Sniper is the priority strategy. When the operator pauses Sniper, exactly one preselected secondary coordinator starts owning new entries: **Scalping / Swing / Expert**. Restoring Sniper returns it to primary ownership. All 22 original pages remain accessible and the original dark institutional dashboard is retained.

## Configuration
- Opt-in `trading.priority_enabled` defaults to **false** on old profiles; legacy manual Scalping/Swing/Expert/Sniper selections keep their exact meaning.
- `trading.sniper_active` defaults true. `fallback_mode` is restricted to `scalping`, `swing` or `expert`, never `sniper`.
- In priority mode, `trading.mode` is the **sole effective execution owner**: Sniper when active, the selected fallback when paused. Inconsistent persisted/API configurations are rejected at validation.
- The native Trading Settings page contains priority enable/disable, backup selector and primary Sniper state. Settings changes are saved only while STOPPED.
- During active monitoring the operator uses **Pause Sniper → Backup** or **Restore Sniper** in the command console or settings page. These call the authenticated local Core `POST /v1/trading/priority/handoff`. Live clients must re-enter their bound REAL login in the Execution START confirmation field.

## Strict handoff while monitoring
Core checks:
1. Priority routing was explicitly enabled and the desired primary state is actually changing.
2. No unreconciled or queued broker commands remain in the durable journal.
3. The connected broker is fresh, matches the bound execution account, and has no **open positions** or **pending orders**.
4. Current quote has a trustworthy timestamp not older than 3 seconds.
5. On REAL, the operator reconfirms the account login and the existing Live portfolio safety controls.
Only then persist the new sole effective mode and reset cached old strategy signals, keeping the existing master START armed. If any guard fails, the old mode remains selected and the interface shows an error.

## Non-goals and risk semantics
- This is an **operator-requested mode handoff**, NOT automatic substitution when no Sniper signal fires, a model is uncertain, or prices are stale. Silence is not a pause event.
- **STOP** is still a global stop of all new broker entries and does not activate the backup.
- Existing broker SL/TP and order reconciliation remain under Core; an open or unresolved trade blocks the mode change rather than passing it blindly to another algorithm.
- There is never more than one MT5 coordinator sending new entries. The inactive mode may continue Advisor/Shadow observation but cannot submit a second broker entry.
- This feature does not disable risk rules, promise better returns, or change account binding.

## Acceptance
`cargo test -p braintrade-domain -p braintrade-runtime --locked` validates config compatibility and exact one-owner invariants.
`python3 scripts/check_priority_handoff.py` boots a real local Core with a synthetic MT5 Demo stream and checks all three secondary modes, operator pause/restore while running, stopped legacy configs, broker-order/position/stale-quote fail-closed behavior, stop/restart and saved-state persistence. No real broker trades are placed.
Windows `scripts/build-windows.ps1` also calls the same API test against packaged `BrainTrade.exe --core`.
A real MT5 Demo session on the operator's PC is the remaining mandatory integration acceptance step.
