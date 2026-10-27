"""Stamp the V25 Windows Tauri binary with a build-identity CLI capability.

Conservatively patch the restored V24 source; never modify V24 branch or REAL settings.
"""
from pathlib import Path
p=Path("apps/tauri/src/main.rs")
s=p.read_text(encoding="utf-8")
needle="fn main() {\n"
stamp="""fn main() {
    if std::env::args().any(|arg| arg == "--braintrade-build-id") {
        eprintln!("BRAINTRADE_V4_ENTERPRISE_V25_P0_20261024");
        return;
    }
"""
if s.count("BRAINTRADE_V4_ENTERPRISE_V25_P0_20261024")==1:
    print("V25 build marker already present")
elif s.count(needle)==1:
    p.write_text(s.replace(needle,stamp,1),encoding="utf-8")
else:
    raise SystemExit("Unrecognized restored Tauri main.rs; refuse automatic build patch")
print("V25 EXE identity stamped in compiled source")
