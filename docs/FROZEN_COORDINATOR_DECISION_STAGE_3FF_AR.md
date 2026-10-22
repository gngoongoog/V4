# Stage 3F-F — Frozen Expert coordinator *decision policy* (Demo only)

## What is actually frozen
This experiment snapshots **the Expert Demo decision policy** at the operator's STOPPED request: selected account/symbol Config, system and subordinate BrainConfig, each advisor's optional Genome, observed LearnedWeight and advisor participation switch, and already learned ExitStudy mappings. The snapshot is immutable in `frozen_coordinator_runs` and survives restart. Only one active snapshot per account/symbol; stopping preserves prior results, and restarting creates another unique run.

At every later freshly observed, **completed M1 bar** (never historical/backfilled), evaluate frozen system market baseline and frozen advisor votes with frozen weights using existing `execution::coordinate_for_account`, then apply frozen `student_learning::adapt`. Record both the frozen policy output and the currently learning SYSTEM output in `frozen_coordinator_decisions`, de-duplicated by completed bar and unique run.

## What it does **not** freeze
- **Not a full execution twin**: the experiment does not reproduce per-account portfolio risk gates, broker margin/commission API, cooldown, in-flight broker commands, idempotency keys, real orders, close position management, live ticks for Sniper, or broker reconciliation. Both signals are **before** the final broker admission gates.
- Frozen advisor membership is the frozen user configuration, not dynamic live position-return exclusions. This is useful for measuring *policy drift* but not exact order parity.
- The runtime `student_learning::adapt` is a reused pure function on frozen ExitStudy data and quote context. The policy snapshot does not train during the experiment. New market quotes are intentionally used for both arms.
- **One bar = one decision observation**. Do not count this as one completed trade or profit. No future returns are calculated in this stage.
- Demo Expert only; REAL and other modes are denied for initiating this study. Freezing requires STOPPED + no pending broker intents, recent bound MT5 Demo snapshot.

## Dashboard and API
The system row in «حالة العقول والتحكم» has Freeze and Stop buttons for a frozen Expert coordinator policy. Show future comparison bars, two ready signals, same/different directions, and frozen-only/adaptive-only readiness. Authorized local API `POST /v1/execution/frozen-coordinator` accepts only `freeze` / `stop` with bound exact account context and mode; GET provides read-only snapshot status. Does not invoke the MT5 Gateway, modify existing risk controls, write into the broker journal or mutate learning weights.

## Verification
Rust synthetic tests: immutability across stop/re-freeze; account isolation; rejection of REAL/non-Expert; old runs not modified. Research/Linux/Windows CI including Slint headless render are required on the final head. Operator must separately test real MT5 Demo trading outcomes and program restart/backup behavior.

The next institutional stage must build a **fully independent simulator of the selected system's orders, fill timing, position state, stops, exits and the entire gate stack** before comparing truly realized trading profitability. This study is a policy-provenance step, not a declaration that such a twin already exists.
