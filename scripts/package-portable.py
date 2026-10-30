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
import tomllib

ROOT = Path(__file__).resolve().parents[1]
VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
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
GATEWAY_FILES = ("mt5_gateway.py", "studio_feed.py", "execution.py", "position_protection.py", "order_cancellation.py", "broker_costs.py", "check_runtime.py", "demo_readiness.py", "sniper_native.py", "institutional_controls.py", "transport.py", "strategy_lab.py")
NATIVE_EA = "BrainTradeSniperBridge.mq5"
OPERATOR_POWERSHELL = "Run-Demo-Acceptance.ps1"
OPERATOR_GUI = "Run-Demo-Acceptance-GUI.ps1"
OPERATOR_CMD = "Run-Demo-Acceptance.cmd"
OPERATOR_DOC = "DEMO_ACCEPTANCE_AR.md"

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
    excluded_top_level = {"target", ".package-cache", ".build-venv", ".ci", ".venv", "venv", "node_modules", "test-output", ".git", "release", "runtime", "build", "dist"}
    excluded_dir_names = {"node_modules", ".venv", "venv", "__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache"}
    secret_suffixes = (".pyc", ".sealed", ".key", ".log", ".sqlite", ".sqlite3", ".db", ".zip", ".7z", ".rar", ".pem", ".p12", ".pfx")
    for path in ROOT.rglob("*"):
        if not path.is_file():
            continue
        relative = path.relative_to(ROOT)
        lower_name = path.name.lower()
        if relative.parts and relative.parts[0] in excluded_top_level:
            continue
        if any(part in excluded_dir_names for part in relative.parts[:-1]):
            continue
        if ".import" in relative.parts:
            continue
        if lower_name.startswith(".env") and lower_name != ".env.example":
            continue
        if lower_name.endswith(secret_suffixes):
            continue
        output = destination / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, output)

def copy_runtime_assets(output):
    """Copy the embedded gateway plus the source-only native MT5 bridge."""
    gateway = output / "runtime/gateway"
    gateway.mkdir(parents=True, exist_ok=True)
    for file in GATEWAY_FILES:
        shutil.copy2(ROOT / "gateway" / file, gateway / file)
    experts = output / "MT5/Experts"
    experts.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / "mt5" / NATIVE_EA, experts / NATIVE_EA)

def copy_operator_assets(output):
    """Expose the target-machine Demo acceptance workflow at package root."""
    tools = output / "Tools"
    tools.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / "scripts" / OPERATOR_POWERSHELL, tools / OPERATOR_POWERSHELL)
    shutil.copy2(ROOT / "scripts" / OPERATOR_GUI, tools / OPERATOR_GUI)
    shutil.copy2(ROOT / "scripts" / OPERATOR_CMD, output / OPERATOR_CMD)
    shutil.copy2(ROOT / "docs" / OPERATOR_DOC, output / OPERATOR_DOC)

def source_commit():
    try:
        return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT,
                                       text=True, stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        return None  # A source-only archive has no Git metadata; never invent provenance.

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
    copy_runtime_assets(output)
    copy_operator_assets(output)
    for source, target in (("BRAIN_CONTROL_SNIPER_AR.md", "BRAIN_CONTROL_SNIPER_AR.md"), ("QUICKSTART_AR.md", "README_AR.md"), ("SNIPER_LEARNER_AR.md", "SNIPER_LEARNER_AR.md"), ("TRY_SNIPER_DEMO_WINDOWS_AR.md", "START_HERE_SNIPER_DEMO_AR.md"), ("RELEASE_STATUS.md", "RELEASE_STATUS.md"), ("THIRD_PARTY.md", "THIRD_PARTY.md")):
        shutil.copy2(ROOT / "docs" / source, output / target)
    source_copy(output / "Source")
    count = collect_licenses(output / "licenses/rust", args.cargo)
    reports = check_pe_tree(output, args.dll_dir)
    (output / "package-validation.json").write_text(json.dumps({"version": VERSION, "architecture": "Windows x64", "source_commit": source_commit(), "rust_dependencies": count, "assets": ASSETS, "native_sniper_bridge": "MT5/Experts/BrainTradeSniperBridge.mq5 (source; target-machine MetaEditor compile/attach still required)", "demo_acceptance_launcher": "Run-Demo-Acceptance.cmd -> Tools/Run-Demo-Acceptance-GUI.ps1 by default; CLI arguments route to Tools/Run-Demo-Acceptance.ps1. Both are read-only collector frontends.", "pe_imports": reports, "validation": "Portable package built from the checked source tree with pinned embedded assets and PE dependency validation. Native MT5 acceptance remains a separate target-machine step, exposed through the packaged read-only GUI/CLI acceptance launchers."}, indent=2), encoding="utf-8")
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
