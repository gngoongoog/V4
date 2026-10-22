# Stage 3F-A — Independent read-only learning quality audit

## Scope
The goal is to make the brain monitor honest about **whether accumulated MT5 Demo evidence is sufficient to describe performance**. It does not approve/promote brains and does not modify any learned weights or broker commands. This is distinct from the existing challenger model performance-review pipeline.

### Evidence segregation
- The input is **only fully closed, broker-confirmed MT5 Demo trades** persisted in `coordinator_broker_samples`. Its payload includes `gross`, `commission`, `swap`, `fee`, `net`, `closed_volume`, `r_multiple`, `brain` and trading mode.
- Validate `net = gross + commission + swap + fee` within conservative floating-point tolerance, with positive volume and finite data. Invalid/incomplete records are counted, **not converted to zero-return labels**.
- An MT5 Demo position credited more than once to the same brain/mode is deduplicated by broker position_id. A trade can be credited to the coordinator and a selected advisor, but those credits are *correlated*, not distinct independent market outcomes.
- Filter by bound MT5 server/login/currency/symbol context, brain and trading mode. Keep a policy/source count; mixed timeframes or strategy source versions are flagged rather than silently pooled as proof.
- **Paper** virtual outcomes, **Shadow**, forward-bar samples and live/unreconciled broker intents are NOT counted as closed Demo fills.

### Temporal inspection, not independent out-of-sample
For the last 240 accepted broker results (max), calculate chronological slices: earlier records and the last 12 outcomes, with a minimum of 30 closed results to show a trend warning. Metrics: booked net, mean net per lot, booked costs drag, positive-after-costs win rate, Wilson 95% lower bound (descriptive binomial interval), profit factor, mean R if documented, and cases where costs erase a positive gross PnL. Null values remain unknown rather than becoming zero.

The last 12 records **are not OOS**: this is a continuously online-learning system, and its weights might already have incorporated those outcomes. A genuine independent OOS stage needs a frozen policy, time-boxed forward period, and separate future data. No automatic promotion or dismissal is implemented at Stage 3F-A.

### UI and audit
The existing Brain Control Room shows the source status and two chronological windows for the selected brain plus validity/duplicate counters. The overview displays the number of valid broker outcomes versus Paper training separately.

## Verification
- No evidence => insufficient_evidence, not "0% win rate".
- Complete, correctly costed 34 trades => 22 earlier / 12 recent and deterioration flag when recent booked net/lot is nonpositive.
- Mixed source labels => mixed_policy_monitor_only.
- Invalid/cross-account/other-mode/other-brain outcomes excluded.
- Run Research/Linux/Windows CI on exact final head. Verify desktop rendering.
- **No funded-account LIVE certification** can be inferred from simulated tests or CI.
