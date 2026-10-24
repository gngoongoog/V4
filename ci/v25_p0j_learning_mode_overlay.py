"""Stage P0-J broker-mode isolated durable research learning over restored V24.

Safeguard: verify exact archived source SHA-256 (normalized to LF) before
installing audited tracked Rust changes. Any source drift blocks publication.
No real broker action, data migration or champion activation.
"""
from pathlib import Path
import hashlib
import subprocess

EXPECTED={
    "crates/runtime/src/learning.rs":"6d109e56216008e5beaa49512fb108b0aa36c1edaf608614bb33f380cef5afce",
    "crates/runtime/src/learning_inference.rs":"dc8507abde37fc13153d92c1b951e0146a307b10f812f477dceac41057d78668",
}
staged={}
for name,old_sha in EXPECTED.items():
    target=Path(name)
    existing=target.read_text(encoding="utf-8")
    digest=hashlib.sha256(existing.encode("utf-8")).hexdigest()
    if digest!=old_sha:
        raise SystemExit(f"P0-J archived learning baseline drift: {name} {digest}; refusing build")
    source=subprocess.check_output(["git","show",f"HEAD:{name}"],text=True,encoding="utf-8")
    if name.endswith("/learning.rs"):
        required=("pub(crate) fn context(t: &Telemetry) -> Option<String>",
                  "fn verified_study_context(raw: &str) -> bool",
                  "same_broker_login_and_bar_remain_separate_after_sqlite_reopen",
                  "training_export_skips_untagged_legacy_rows_and_preserves_them_in_db")
    else:
        required=("crate::learning::context(telemetry)",)
    if not all(marker in source for marker in required):
        raise SystemExit(f"P0-J tracked Rust source missing required contract: {name}")
    staged[target]=source
for path,data in staged.items():
    path.write_text(data,encoding="utf-8")
    print(f"P0-J verified learning/isolation source staged: {path}")
