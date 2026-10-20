"""Run the packaged Windows executable/Core with isolated temporary app data.
Uses Windows DPAPI for the same IPC key format as the real desktop.
Does not configure Telegram, submit orders, or touch existing application data.
"""
from __future__ import annotations
import argparse
import ctypes
from ctypes import wintypes
import json
import os
import shutil
from pathlib import Path
import subprocess
import tempfile
import time
import tomllib
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]


def unseal(path: Path) -> str:
    class Blob(ctypes.Structure):
        _fields_ = [("size", wintypes.DWORD), ("data", ctypes.POINTER(ctypes.c_ubyte))]
    raw = path.read_bytes()
    memory = (ctypes.c_ubyte * len(raw)).from_buffer_copy(raw)
    source, output = Blob(len(raw), memory), Blob()
    crypt = ctypes.WinDLL("crypt32", use_last_error=True)
    crypt.CryptUnprotectData.argtypes = [ctypes.POINTER(Blob), ctypes.c_void_p, ctypes.c_void_p,
                                        ctypes.c_void_p, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(Blob)]
    crypt.CryptUnprotectData.restype = wintypes.BOOL
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.LocalFree.argtypes = [ctypes.c_void_p]
    kernel.LocalFree.restype = ctypes.c_void_p
    if not crypt.CryptUnprotectData(ctypes.byref(source), None, None, None, None, 0, ctypes.byref(output)):
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        return ctypes.string_at(output.data, output.size).decode("utf-8")
    finally:
        kernel.LocalFree(output.data)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=Path("test-output/windows-portable-smoke.json"))
    args = parser.parse_args()
    if os.name != "nt":
        raise SystemExit("This acceptance harness requires Windows")
    executable = args.exe.resolve()
    package_root = executable.parent
    native_runtime = package_root / "runtime/gateway/sniper_native.py"
    native_ea = package_root / "MT5/Experts/BrainTradeSniperBridge.mq5"
    assert native_runtime.is_file(), native_runtime
    assert native_ea.is_file(), native_ea
    ea_source = native_ea.read_text(encoding="utf-8")
    assert "void OnTick()" in ea_source and "FileFlush(g_pipe)" in ea_source
    assert "OrderSend" not in ea_source and "CTrade" not in ea_source

    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with tempfile.TemporaryDirectory(prefix="braintrade-windows-check-") as folder:
        directory = Path(folder)
        environment = dict(os.environ, BRAINTRADE_DATA_DIR=folder, BRAINTRADE_NEWS_OFFLINE="1")
        environment.pop("BRAINTRADE_GATEWAY_SCRIPT", None)
        with (directory / "native-stderr.log").open("wb") as log:
            process = subprocess.Popen([str(executable), "--core"], env=environment, stderr=log)
            key = ""
            def api(path, body=None):
                request = urllib.request.Request("http://127.0.0.1:18741" + path,
                    data=None if body is None else json.dumps(body).encode(),
                    headers={"Authorization": "Bearer " + key, "Content-Type": "application/json"})
                with opener.open(request, timeout=5) as response:
                    return json.load(response)
            try:
                for _ in range(100):
                    try:
                        key = unseal(directory / "ipc.key")
                        status = api("/v1/status")
                        break
                    except (OSError, ValueError):
                        if process.poll() is not None:
                            raise AssertionError("Packaged Core exited before startup")
                        time.sleep(0.1)
                else:
                    raise AssertionError("Packaged Core startup timeout")
                assert status["version"] == VERSION and status["state"] == "stopped", status
                assert status["execution_enabled"] is False
                assert api("/v1/learning")["champion"] is None
                student = api("/v1/execution/student")
                assert student["total_experiences"] == 0 and student["closed_broker_trades"] == 0
                assert student["demo_connected"] is False
                assert api("/v1/opportunities/history")["setups"] == []
                assert api("/v1/stop", {})["state"] == "stopped"
                config = api("/v1/config")["config"]
                api("/v1/config", {"config": config})
                assert len(api("/v1/studio")["frames"]) == 21
                for mode in ("scalping", "swing", "expert", "sniper"):
                    trading = dict(config["trading"], mode=mode)
                    api("/v1/trading/settings", {"trading": trading, "commission_per_lot": 7.0})
                    assert api("/v1/config")["config"]["trading"]["mode"] == mode
                fees = api("/v1/trading/preview", {"lots": 0.1, "commission_per_lot": 7.0, "pip_points": 10.0})
                assert abs(fees["commission"] - 0.70) < 1e-10
                assert "costs" not in fees

                assert (directory / "config.json").is_file()
                risk = dict(config["risk"])
                risk["risk_percent"] = 0.31
                api("/v1/risk", risk)
                backup = Path(api("/v1/backup", {})["path"])
                assert (backup / "events.sqlite").is_file()
                assert api("/v1/backup/restore/validate", {"name": backup.name})["valid"]
                risk["risk_percent"] = 0.42
                api("/v1/risk", risk)
                original_key = (directory / "ipc.key").read_bytes()
                staged = api("/v1/backup/restore/stage", {"name": backup.name})
                assert staged["staged"] and staged["secrets_restored"] is False
                assert process.wait(timeout=15) == 0
                process = subprocess.Popen([str(executable), "--core"], env=environment, stderr=log)
                for _ in range(100):
                    try:
                        status = api("/v1/status")
                        break
                    except OSError:
                        if process.poll() is not None:
                            raise AssertionError("Packaged Core exited during restore")
                        time.sleep(0.1)
                else:
                    raise AssertionError("Packaged Core restore timeout")
                assert status["state"] == "stopped" and not status["execution_enabled"]
                assert api("/v1/config")["config"]["risk"]["risk_percent"] == 0.31
                assert api("/v1/config")["config"]["trading"]["mode"] == "sniper"
                restored = api("/v1/backup/restore/status")
                assert restored["pending"] is False and restored["last_result"]["status"] == "applied"
                assert (directory / "ipc.key").read_bytes() == original_key
                assert all(item["valid"] for item in api("/v1/backups")["backups"])
                api("/v1/shutdown", {})
                assert process.wait(timeout=15) == 0
                subprocess.run([str(executable), "--render-check"], env=environment,
                               cwd=directory, check=True, timeout=180)
                frames = directory / "test-output" / "native-ui"
                expected = {}
                for width, height, suffix in [(1120, 820, ""), (880, 700, "-small")]:
                    for page in range(21):
                        expected[f"dashboard-page-{page}{suffix}.ppm"] = (width, height)
                observed = {path.name for path in frames.glob("*.ppm")}
                required = set(expected)
                assert observed == required, f"Render output mismatch: missing={sorted(required-observed)} unexpected={sorted(observed-required)} render_directory={frames}"
                for name, (width, height) in expected.items():
                    raw = (frames / name).read_bytes()
                    header = f"P6\n{width} {height}\n255\n".encode()
                    assert raw.startswith(header) and len(raw) == len(header) + width * height * 3, name
                args.output.parent.mkdir(parents=True, exist_ok=True)
                shutil.copytree(frames, args.output.parent / "native-ui", dirs_exist_ok=True)
                args.output.write_text(json.dumps({"version": status["version"], "result": "PASS",
                    "checks": ["packaged Core startup", "DPAPI IPC", "stopped/unbound default",
                               "native Sniper runtime and source-only MQL5 EA packaged", "MQL5 bridge contains no order submission API",
                               "learning and student journal APIs", "four mode settings and restart", "proportional fee calculator", "21-frame studio API", "opportunity history", "config save", "backup", "staged restore and restart", "rollback snapshot", "DPAPI key preserved", "shutdown", "all 42 complete native frames"],
                    "native_pages": 21, "rendered_frames": len(expected),
                    "native_mt5_acceptance": "EA source packaged; target-machine MetaEditor compile/attach and real OnTick pipe acceptance not performed"}, indent=2), encoding="utf-8")
            finally:
                if process.poll() is None:
                    try:
                        api("/v1/shutdown", {})
                        process.wait(timeout=10)
                    except Exception:
                        subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"], check=False,
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                if process.returncode not in (None, 0):
                    print((directory / "native-stderr.log").read_text(encoding="utf-8", errors="replace"))

if __name__ == "__main__":
    main()
