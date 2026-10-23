"""Static V23-compatible supervisor wiring contract, not evidence of broker execution."""
from pathlib import Path
root=Path(__file__).resolve().parents[1]
files=["crates/runtime/src/enterprise_supervisor.rs","crates/runtime/src/execution.rs","crates/runtime/src/lib.rs","apps/tauri/src/bridge.rs","apps/terminal/src/enterprise-supervisor.tsx","apps/terminal/src/bridge.ts"]
for name in files:
    p=root/name
    assert p.is_file() and p.stat().st_size>500,(name,"missing/empty")
core=(root/"crates/runtime/src/lib.rs").read_text(encoding="utf-8")
exe=(root/"crates/runtime/src/execution.rs").read_text(encoding="utf-8")
api=(root/"apps/tauri/src/bridge.rs").read_text(encoding="utf-8")
checks={
  "supervisor read/write API": "/v1/enterprise/supervisor" in core,
  "persist before acknowledgement": "enterprise_supervisor::save(&self.db" in core,
  "normal path validates persisted policy": "let entries_allowed=if run && self.supervisor_policy_valid {" in exe,
  "fast pulse validates persisted policy": "let enter=if status.requested_run && self.supervisor_policy_valid {" in exe,
  "both paths respect entry gate": exe.count("self.supervisor.enforce_entry_gate") >= 3,
  "open-position management preserved": "self.inner\n            .observe(db, telemetry, config, entries_allowed, frames, now)?" in exe,
  "Tauri path allowlist": "/v1/enterprise/supervisor" in api and "fn update_supervisor" in api,
  "Rust unit tests present": "fn stale_price_never_becomes_new_with_fresh_heartbeat" in (root/files[0]).read_text(encoding="utf-8"),
}
for label,good in checks.items():print(("PASS" if good else "FAIL"),label)
assert all(checks.values()), "V23 supervisor wire contract failed"
print("V23 static supervisor wiring PASS; does not certify Rust build or MT5 trades")
