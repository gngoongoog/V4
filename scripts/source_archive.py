"""Create an atomic source-only ZIP. No runtime data, build outputs or secrets."""
from pathlib import Path
import hashlib
import os
import sys
import zipfile

root = Path(__file__).resolve().parents[1]
dest = Path(sys.argv[1]).resolve()
excluded = {'target', '.package-cache', '.build-venv', 'test-output', '__pycache__', '.git', 'release', 'build', 'dist'}
suffixes = {'.pyc', '.sealed', '.key', '.log', '.sqlite', '.zip'}
temp = dest.with_suffix('.tmp')
with zipfile.ZipFile(temp, 'w', zipfile.ZIP_DEFLATED) as archive:
    for p in sorted(root.rglob('*')):
        rel = p.relative_to(root)
        if p.is_file() and not set(rel.parts) & excluded and p.suffix not in suffixes:
            archive.write(p, Path('BrainTrade-OS') / rel)
with temp.open('rb') as handle:
    os.fsync(handle.fileno())
os.replace(temp, dest)
directory_fd = os.open(dest.parent, os.O_DIRECTORY)
os.fsync(directory_fd)
os.close(directory_fd)
os.sync()
print(dest, dest.stat().st_size, hashlib.sha256(dest.read_bytes()).hexdigest())
