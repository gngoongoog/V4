"""Install the tracked, reviewed V25 Core fill-state fix over restored V24 CI source.

Never replace the entire reconstructed Rust file. Transfer only anchored and
accounted-for helper, route and tests; unexpected source layouts abort the build.
"""
from pathlib import Path
import subprocess

p = Path("crates/runtime/src/execution_legacy.rs")
s = p.read_text(encoding="utf-8")
tracked = subprocess.check_output(
    ["git", "show", "HEAD:crates/runtime/src/execution_legacy.rs"], text=True
)

def segment(doc, start, end):
    if doc.count(start) != 1 or doc.count(end) != 1:
        raise SystemExit("V25 Core patch segment markers unexpected; fail closed")
    return doc[doc.index(start):doc.index(end)]

helper = segment(tracked,
    "/// Only monotonic, broker-history-proven split fills",
    "impl Service {")
new_route = segment(tracked,
    "                let (old, previous_json): (String, String) = db.query_row(",
    "                Ok(json!({\"ok\":true}))")
new_tests = segment(tracked,
    "    #[test]\n    fn recovered_partial_progress_requires_exact_volume_and_new_broker_deals() {",
    "    #[test]\n    fn independent_daily_breaker_latches_across_symbols_but_resets_next_baghdad_day() {")
old_route = """                let old: String =
                    db.query_row("SELECT state FROM broker_commands WHERE id=?1", [id], |r| {
                        r.get(0)
                    })?;
                if matches!(old.as_str(), "dispatched" | "unknown" | "placed") {
                    db.execute(
                        "UPDATE broker_commands SET state=?2,result=?3 WHERE id=?1",
                        params![id, state, v.to_string()],
                    )?;
                }
"""
old_tests = "    #[test]\n    fn independent_daily_breaker_latches_across_symbols_but_resets_next_baghdad_day() {\n"
assert "valid_recovered_partial_progress" in helper and "else if matches!" in new_route
if "fn valid_recovered_partial_progress" in s:
    if "let (old, previous_json)" not in s or "fn late_split_fill_updates_core_result" not in s:
        raise SystemExit("Partially applied V25 Core patch")
    print("V25 Core progression patch already present")
else:
    if (s.count("use std::collections::BTreeMap;") != 1
            or s.count(old_route) != 1
            or s.count(old_tests) != 1
            or s.count("impl Service {") != 1):
        raise SystemExit("Unexpected reconstructed Rust Core; refusing to replace V24 source")
    s = s.replace("use std::collections::BTreeMap;",
                  "use std::collections::{BTreeMap, BTreeSet};", 1)
    s = s.replace("impl Service {", helper + "impl Service {", 1)
    s = s.replace(old_route, new_route, 1)
    s = s.replace(old_tests, new_tests + old_tests, 1)
    p.write_text(s, encoding="utf-8")
print("V25 broker Core result progression code staged, no mode or advisor changed")
