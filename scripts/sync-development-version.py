from __future__ import annotations

import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
PREVIOUS = "0.6.0"


def replace(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    if old in text:
        target.write_text(text.replace(old, new), encoding="utf-8")
        return
    if new not in text:
        raise RuntimeError(f"version sync anchor missing in {path}: {old!r}")


# Current-development UI only. Historical release records are intentionally untouched.
replace("crates/desktop/ui/main.slint", f"Research {PREVIOUS}", f"Research {VERSION}")
replace("crates/desktop/ui/main.slint", f"RESEARCH {PREVIOUS}", f"RESEARCH {VERSION}")
replace("crates/desktop/ui/main.slint", f"BRAINTRADE OS {PREVIOUS}", f"BRAINTRADE OS {VERSION}")

replace(
    "docs/EXECUTION_AND_ROLES.md",
    f"# BrainTrade OS {PREVIOUS} — broker execution and membership",
    f"# BrainTrade OS {VERSION} Development Preview — broker execution and membership",
)
replace(
    "docs/TRADING_MODES_AND_STUDIO.md",
    f"# Trading modes and learning studio — {PREVIOUS}",
    f"# Trading modes and learning studio — {VERSION} Development Preview",
)
replace(
    "docs/QUICKSTART_AR.md",
    f"# BRAINTRADE OS {PREVIOUS} Preview — التشغيل على Windows",
    f"# BRAINTRADE OS {VERSION} Development Preview — التشغيل على Windows",
)
replace(
    "docs/QUICKSTART_AR.md",
    f"C:\\BrainTrade-{PREVIOUS}",
    f"C:\\BrainTrade-{VERSION}",
)
replace(
    "docs/ARCHITECTURE.md",
    f"Current release candidate: **{PREVIOUS} Preview**. Cargo, lockfile, desktop and Windows package versions are aligned. The user has requested Windows delivery; use BUILD_HANDOFF.md and docs/validation.json for the exact final validation boundary.",
    f"Promoted release: **{PREVIOUS}**. Current development: **{VERSION} Development Preview**. The development Cargo, lockfile, desktop and Windows package versions must stay aligned. Use docs/validation.json for promoted-release evidence and docs/development-validation.json for the exact current-development validation boundary.",
)

print(f"Synchronized current-development version text to {VERSION}; historical release evidence unchanged.")
