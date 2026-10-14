"""Create an atomic source-only ZIP. No runtime data, build outputs or secrets."""
from __future__ import annotations

from pathlib import Path
import hashlib
import os
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[1]
EXCLUDED_TOP_LEVEL = {
    "target",
    ".package-cache",
    ".build-venv",
    "test-output",
    ".git",
    "release",
    "runtime",
    "build",
    "dist",
}
SECRET_SUFFIXES = {
    ".pyc",
    ".sealed",
    ".key",
    ".log",
    ".sqlite",
    ".sqlite3",
    ".db",
    ".zip",
    ".7z",
    ".rar",
    ".pem",
    ".p12",
    ".pfx",
}


def source_files(root: Path):
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        relative = path.relative_to(root)
        lower_name = path.name.lower()
        if relative.parts and relative.parts[0] in EXCLUDED_TOP_LEVEL:
            continue
        if ".import" in relative.parts:
            continue
        if lower_name.startswith(".env") and lower_name != ".env.example":
            continue
        if path.suffix.lower() in SECRET_SUFFIXES:
            continue
        yield path, relative


def create_archive(root: Path, destination: Path) -> tuple[int, str]:
    root = root.resolve()
    destination = destination.resolve()
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_suffix(destination.suffix + ".tmp")
    if temporary.exists():
        temporary.unlink()
    with zipfile.ZipFile(temporary, "w", zipfile.ZIP_DEFLATED) as archive:
        for path, relative in source_files(root):
            archive.write(path, Path("BrainTrade-OS") / relative)
    with temporary.open("rb+") as handle:
        handle.flush()
        os.fsync(handle.fileno())
    temporary.replace(destination)
    if os.name != "nt":
        directory_fd = os.open(destination.parent, os.O_RDONLY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    with zipfile.ZipFile(destination) as archive:
        bad = archive.testzip()
        if bad is not None:
            raise ValueError(f"Corrupt source archive member: {bad}")
    digest = hashlib.sha256(destination.read_bytes()).hexdigest()
    return destination.stat().st_size, digest


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: source_archive.py OUTPUT.zip")
    destination = Path(sys.argv[1])
    size, sha256 = create_archive(ROOT, destination)
    print(destination.resolve(), size, sha256)


if __name__ == "__main__":
    main()
