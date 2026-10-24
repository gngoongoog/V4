"""P0-N offline baseline trainer source overlay for reconstructed V25 Windows CI.

Restored V24 trainer is accepted only if its pre-certification input boundary
is present exactly once. New code is taken from the same checked-out commit;
all inconsistent states fail closed rather than packaging a stale trainer.
"""
from pathlib import Path
import subprocess

path=Path("scripts/train_linear_baseline.py")
old=path.read_text(encoding="utf-8")
if old.count("def load_dataset(path, minutes):") != 1 or "def certified_manifest(" in old:
    raise SystemExit("P0-N archived trainer source unexpectedly modified")
tracked=subprocess.check_output(["git","show","HEAD:scripts/train_linear_baseline.py"],text=True,encoding="utf-8")
requirements=(
    "def verified_broker_context(raw):",
    "def certified_manifest(dataset):",
    "def load_dataset(path, minutes, *, certified=False):",
    "load_dataset(args.dataset, args.target_minutes, certified=True)",
)
if not all(text in tracked for text in requirements):
    raise SystemExit("P0-N checked-out trainer missing certified training contract")
path.write_text(tracked,encoding="utf-8")
print("P0-N verified offline trainer broker mode/manifest audit staged for Windows")
