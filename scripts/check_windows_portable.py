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
from pathlib import Path
import subprocess
import tempfile
import time
import urllib.request


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
                assert status["version"] == "0.4.0" and status["state"] == "stopped", status
                assert status["execution_enabled"] is False
                assert api("/v1/learning")["champion"] is None
                assert api("/v1/opportunities/history")["setups"] == []
                assert api("/v1/stop", {})["state"] == "stopped"
                config = api("/v1/config")["config"]
                api("/v1/config", {"config": config})
                assert (directory / "config.json").is_file()
                backup = Path(api("/v1/backup", {})["path"])
                assert (backup / "events.sqlite").is_file()
                api("/v1/shutdown", {})
                assert process.wait(timeout=15) == 0
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_text(json.dumps({"version": status["version"], "result": "PASS",
                    "checks": ["packaged Core startup", "DPAPI IPC", "stopped/read-only default",
                               "learning API", "opportunity history", "config save", "backup", "shutdown"],
                    "native_mt5_acceptance": "not performed"}, indent=2), encoding="utf-8")
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
