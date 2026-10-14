"""Build a Windows x64 portable preview with isolated Python and complete source.
Run on a build host with Python 3.11+, pefile and a compiled BrainTrade.exe.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
VERSION = "0.3.0"
ASSETS = [
    {"file": "python-3.13.16-embed-amd64.zip", "url": "https://www.python.org/ftp/python/3.13.16/python-3.13.16-embed-amd64.zip",
     "sha256": "97dae5274cc54867065e8d5a3226e48c35017ed332a0fdb0e27d5b5821961297", "destination": "runtime/python"},
    {"file": "metatrader5-5.0.5488-cp313-cp313-win_amd64.whl",
     "url": "https://files.pythonhosted.org/packages/89/9c/f2714fe36d063c4e0c8f5b5f95b88b5c6b7c8777b294482f5d37ef43b142/metatrader5-5.0.5488-cp313-cp313-win_amd64.whl",
     "sha256": "9c117b4fb9f6f05e924a37069e9c6a151d16594519f30dbf4ec0d548131f540d", "destination": "runtime/python/Lib/site-packages"},
    {"file": "numpy-2.2.6-cp313-cp313-win_amd64.whl",
     "url": "https://files.pythonhosted.org/packages/cb/3b/d58c12eafcb298d4e6d0d40216866ab15f59e55d148a5658bb3132311fcf/numpy-2.2.6-cp313-cp313-win_amd64.whl",
     "sha256": "b0544343a702fa80c95ad5d3d608ea3599dd54d4632df855e4c8d24eb6ecfa1c", "destination": "runtime/python/Lib/site-packages"},
]
SYSTEM_DLLS = set("""advapi32 avrt bcrypt bcryptprimitives cfgmgr32 comctl32 comdlg32 crypt32 cryptbase cryptsp d2d1 d3d11 d3d12 dcomp dbghelp dhcpcsvc dnsapi dwmapi dwrite dxgi gdi32 gdiplus hid imm32 iphlpapi kernel32 kernelbase msimg32 msvcrt ncrypt normaliz ntdll ole32 oleacc oleaut32 opengl32 powrprof propsys psapi rpcrt4 secur32 setupapi shell32 shlwapi user32 userenv usp10 uxtheme version windowscodecs winhttp wininet winmm winscard winspool wintrust ws2_32 wtsapi32 ucrtbase mswsock ntmarta shcore combase netapi32 sensapi wldap32""".split())

def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()

def build_manifest(folder):
    # A previous manifest must never hash itself when staging is reused.
    return {p.relative_to(folder).as_posix(): digest(p)
            for p in sorted(folder.rglob("*"))
            if p.is_file() and p != folder / "SHA256SUMS.json"}

def extract_checked(archive, dest):
    dest.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive) as z:
        for entry in z.infolist():
            relative = PurePosixPath(entry.filename)
            if relative.is_absolute() or ".." in relative.parts or "\\" in entry.filename or ":" in entry.filename:
                raise ValueError("Unsafe archive member")
            if (entry.external_attr >> 16) & 0o170000 == 0o120000:
                raise ValueError("Archive symlinks unsupported")
            z.extract(entry, dest)

def download(asset, cache):
    path = cache / asset["file"]
    if not path.exists():
        temporary = path.with_suffix(path.suffix + ".part")
        with urllib.request.urlopen(asset["url"], timeout=60) as source, temporary.open("wb") as output:
            shutil.copyfileobj(source, output)
        if digest(temporary) != asset["sha256"]:
            temporary.unlink()
            raise ValueError("Downloaded asset hash mismatch")
        temporary.replace(path)
    if digest(path) != asset["sha256"]:
        raise ValueError("Cached asset hash mismatch")
    return path

def collect_licenses(destination, cargo):
    metadata = json.loads(subprocess.check_output([cargo, "metadata", "--locked", "--format-version", "1", "--filter-platform", "x86_64-pc-windows-msvc"], cwd=ROOT))
    catalog = []
    for package in metadata["packages"]:
        if not package["source"]:
            continue
        base = Path(package["manifest_path"]).parent
        folder = destination / (package["name"] + "-" + package["version"])
        notices = []
        for source in base.rglob("*"):
            if not source.is_file():
                continue
            relative = source.relative_to(base)
            if len(relative.parts) > 3 or not any(part.lower().startswith(("license", "licence", "copying", "copyright", "notice", "authors")) for part in relative.parts):
                continue
            output = folder / relative
            output.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, output)
            notices.append(str(relative))
        catalog.append({"name": package["name"], "version": package["version"], "license": package["license"], "repository": package["repository"], "notice_files": notices})
    destination.mkdir(parents=True, exist_ok=True)
    (destination / "dependency-catalog.json").write_text(json.dumps(catalog, indent=2), encoding="utf-8")
    return len(catalog)

def check_pe_tree(folder, extra_dirs=()):
    import pefile
    binaries = [p for p in folder.rglob("*") if p.suffix.lower() in (".exe", ".dll", ".pyd")]
    shipped = {p.name.lower(): p for p in binaries}
    available = {p.name.lower(): p for d in extra_dirs for p in Path(d).glob("*.dll")}
    pending, reports, missing = list(binaries), [], set()
    while pending:
        path = pending.pop()
        pe = pefile.PE(str(path), fast_load=True)
        pe.parse_data_directories(directories=[pefile.DIRECTORY_ENTRY['IMAGE_DIRECTORY_ENTRY_IMPORT'], pefile.DIRECTORY_ENTRY['IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT']])
        if pe.FILE_HEADER.Machine != 0x8664:
            raise ValueError("Non-x64 PE file: " + str(path))
        entries = getattr(pe, "DIRECTORY_ENTRY_IMPORT", []) + getattr(pe, "DIRECTORY_ENTRY_DELAY_IMPORT", [])
        imports = sorted({entry.dll.decode("ascii").lower() for entry in entries})
        for name in imports:
            if name.startswith(("api-ms-win-", "ext-ms-win-")) or name.removesuffix(".dll") in SYSTEM_DLLS:
                continue
            if name not in shipped and name in available:
                target = folder / name
                shutil.copy2(available[name], target)
                shipped[name] = target
                pending.append(target)
            if name not in shipped:
                missing.add((str(path.relative_to(folder)), name))
        reports.append({"file": str(path.relative_to(folder)), "imports": imports})
        pe.close()
    if missing:
        raise ValueError("Unresolved non-system DLLs: " + repr(sorted(missing)))
    return reports

def source_copy(destination):
    excluded = {"target", ".package-cache", ".build-venv", "test-output", "__pycache__", ".git", "release", "runtime", "build", "dist"}
    secret_suffixes = (".pyc", ".sealed", ".key", ".log", ".sqlite", ".sqlite3", ".db", ".pem", ".p12", ".pfx")
    for path in ROOT.rglob("*"):
        if not path.is_file():
            continue
        relative = path.relative_to(ROOT)
        lower_name = path.name.lower()
        if excluded.intersection(relative.parts):
            continue
        if lower_name.startswith(".env") and lower_name != ".env.example":
            continue
        if lower_name.endswith(secret_suffixes):
            continue
        output = destination / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, output)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--dll-dir", action="append", type=Path, default=[])
    args = parser.parse_args()
    if not args.exe.is_file():
        raise SystemExit("Compiled BrainTrade.exe is required")
    cache = ROOT / ".package-cache"
    cache.mkdir(exist_ok=True)
    output = ROOT / "release/BrainTrade"
    if output.exists():
        shutil.rmtree(output)
    output.mkdir(parents=True)
    shutil.copy2(args.exe, output / "BrainTrade.exe")
    for asset in ASSETS:
        extract_checked(download(asset, cache), output / asset["destination"])
    (output / "runtime/python/python313._pth").write_text("python313.zip\n.\nLib/site-packages\n../gateway\nimport site\n", encoding="ascii")
    (output / "runtime/gateway").mkdir(parents=True)
    for file in ("mt5_gateway.py", "check_runtime.py"):
        shutil.copy2(ROOT / "gateway" / file, output / "runtime/gateway" / file)
    for source, target in (("QUICKSTART_AR.md", "README_AR.md"), ("RELEASE_STATUS.md", "RELEASE_STATUS.md"), ("THIRD_PARTY.md", "THIRD_PARTY.md")):
        shutil.copy2(ROOT / "docs" / source, output / target)
    source_copy(output / "Source")
    count = collect_licenses(output / "licenses/rust", args.cargo)
    reports = check_pe_tree(output, args.dll_dir)
    (output / "package-validation.json").write_text(json.dumps({"version": VERSION, "architecture": "Windows x64", "rust_dependencies": count, "assets": ASSETS, "pe_imports": reports, "validation": "Portable package built from the checked source tree with pinned embedded assets and PE dependency validation. Native MT5 acceptance remains a separate manual step."}, indent=2), encoding="utf-8")
    manifest = build_manifest(output)
    (output / "SHA256SUMS.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    archive = ROOT / "release" / f"BrainTrade-Windows-x64-v{VERSION}-Preview.zip"
    temporary_archive = archive.with_suffix(".zip.tmp")
    with zipfile.ZipFile(temporary_archive, "w", zipfile.ZIP_DEFLATED, compresslevel=6, strict_timestamps=False) as z:
        for path in sorted(output.rglob("*")):
            if path.is_file():
                z.write(path, Path("BrainTrade") / path.relative_to(output))
    with temporary_archive.open("rb+") as completed:
        completed.flush()
        os.fsync(completed.fileno())
    temporary_archive.replace(archive)
    if os.name != "nt":
        directory_fd = os.open(archive.parent, os.O_RDONLY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    with zipfile.ZipFile(archive) as z:
        if z.testzip() is not None:
            raise ValueError("Corrupt output ZIP")
        expected = {"BrainTrade/" + name for name in manifest} | {"BrainTrade/SHA256SUMS.json"}
        if len(z.namelist()) != len(expected) or set(z.namelist()) != expected:
            raise ValueError("Archive member coverage mismatch")
        for name, expected_hash in manifest.items():
            if hashlib.sha256(z.read("BrainTrade/" + name)).hexdigest() != expected_hash:
                raise ValueError("Archive content hash mismatch: " + name)
    print(json.dumps({"archive": str(archive), "bytes": archive.stat().st_size, "sha256": digest(archive), "files": len(manifest)+1}))

if __name__ == "__main__":
    main()
