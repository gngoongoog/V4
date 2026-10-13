# Native Windows + MT5 acceptance — pending

These checks require a Windows x64 host with a real MT5 installation and a demo
account. They are not represented as passed by Linux tests.

- Build portable package on Windows with the locked Cargo dependencies.
- Run from a path with spaces and Arabic characters; no terminal window required.
- Use native file picker to select the actual terminal64.exe.
- Sign into a demo account inside MT5. Close MT5, open BrainTrade, verify that the
  gateway launches the selected terminal and reports the matching account.
- Check positions and pending orders against MT5 including an account with zero
  positions. Simulate read failures: unknown state must not become an empty book.
- Check a server disconnect and reconnect; no stale balance displayed as current.
- Switch MT5 account during collection; reject a mixed account snapshot.
- Close the desktop and reopen; core PID and audit continuity remain unchanged.
- Launch twice; only one core and gateway; two UI windows may view the same core.
- Stop; verify no new virtual entries, existing virtual exits continue, and reports remain available.
- Open Analysis; match the symbol and closed M1/M5/M15 candles with the broker.
  Missing history, stale quotes or disagreement must show WAIT with a reason.
- Open Shadow; observe one eligible virtual entry per closed M1 bar. Compare the
  entry/exit bid/ask and configured costs with the exported decision/event data.
- Restart the core; virtual equity, open position and limits must persist while
  new entries remain stopped until START. Downtime must be flagged, not filled in.
- Save risk settings and reopen; values persist. Saving MT5 settings must retain
  risk settings, and changing risk values must not clear an already latched limit.
- Export; verify candles, decisions and paper events belong to the expected
  account/server/symbol. These exports contain account identifiers.
- Full Exit; no remaining gateway/core; terminal and broker positions unchanged.
- Enter bot token locally; inspect config/audit/backups for absence of plaintext.
- Reopen under same Windows user; DPAPI secret loads; mismatched user cannot decrypt.
- Verify private authorized Telegram report, keyboard and all read-only commands.
- Verify a different user and a group receive no account information.
- Disable/change token while a long poll is active; obsolete responses ignored.
- Compare daily deal net including commission, swap and fees across Baghdad midnight.
- Build backup while the core runs; SQLite integrity check succeeds.
- Read Arabic labels on Windows; verify font shaping, file picker, resizing and
  settings scrolling at 100%, 125%, 150% display scaling.

Future broker execution, portfolio risk, MQL5 transport, backtest, learning, full restore and
multi-account capabilities require separate acceptance plans before activation.



## 0.3.0 source additions — native checks for later

Windows packaging is deferred. These checks are for the eventual runnable build,
not a request to test now or a claim they already passed on Windows.

- Start with the previous core fully exited; MT5 settings and shadow state persist.
- In the historical lab, STOP first, choose a short completed UTC range and Meta.
  Verify actual broker history coverage and account/symbol identity in the report.
- Open/export that result; disconnect MT5 and replay it. Identical frozen inputs
  must give the same report. Run CES on the saved input as a separate experiment.
- The single source of the displayed metrics must be the displayed report ID;
  selecting an older result should not relabel the new result's metrics.
- Check News source times, disable/re-enable updates, and open a source link.
- Configure the bot locally when ready; test /news and /backtest from the
  allowlisted private user only. No bot delivery is claimed by local unit tests.
- No new code path should enable broker orders. Execution remains false.
