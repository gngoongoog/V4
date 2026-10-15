# Continue BrainTrade OS — 0.6.0 execution work in progress

Canonical repository gngoongoog/V4, active branch development. User explicitly authorized actual Demo execution, later manual Live selection, independent brain controls and removal of optional portfolio/strategy risk restrictions on 2026-10-15. Preserve transport identity/idempotence and broker contract correctness.

Current checkpoint adds 14 independent strategy identities, durable Core broker intents, a separate non-restored gateway send journal, Demo/Live binding, broker-owned closing/reconciliation, per-brain results and a native Execution page. Optional risk limits default off. News and statistical challengers remain research sources without fabricated standalone trading rules.

Local Core/domain test compilation and desktop compilation pass; 12 fake-MT5 execution tests pass. Full integration, rejection/partial-fill/account switching/restore tests, Windows package and final delivery are still IN PROGRESS. Do not present this checkpoint as a verified release. Last delivered Windows version is 0.5.0 at a5961ff plus documentation commit 187e327.

Next: real-Core execution harness and ownership/latency coverage, update source contracts/version/docs, build all native pages and Windows CI, inspect package, save final download. All current broker testing uses synthetic MT5 responses, not the user's broker account.

## Latest 2026-10-15 role clarification (work in progress)

Continue from the fetched Git head, which includes coordinator wrappers from 9de48ad and earlier changes. The user requires actual exclusion from coordinator voting in SOLO, and rejoining on return. Dynamic membership replaces the former fixed advisor array. Returning members with owned positions or unsettled intents wait before rejoining. Expert chooses direction from member evidence; weights learn from causal cost-adjusted 12-native-frame-bar outcomes, neutral until 20 samples. Sniper now triggers from a live quote crossing the last six completed M1 bars, without waiting for the current candle to close. Windows version/docs/CI still need updating; no new delivery is yet verified.

Recovered transport fixes: fill visibility barrier, manual-exit ownership from deal history, timeout reconciliation without replay, netting in-flight serialization, and isolated gateway-disabled test environment. Finish integration harness, native controls/rendering, docs and Windows validation. Preserve separate gateway journal across restore.
