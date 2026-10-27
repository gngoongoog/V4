"""Real Core + harmless process fixtures; no MetaTrader or broker commands."""
from __future__ import annotations

import ctypes
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "debug" / ("braintrade-core.exe" if os.name == "nt" else "braintrade-core")
sys.path.insert(0, str(ROOT / "gateway"))
from mt5_gateway import lock_gateway


def read_ipc_key(path):
    data = path.read_bytes()
    if os.name != "nt":
        return data.decode()
    from ctypes import wintypes
    class Blob(ctypes.Structure):
        _fields_ = [("cbData", wintypes.DWORD), ("pbData", ctypes.POINTER(ctypes.c_ubyte))]
    source = ctypes.create_string_buffer(data)
    input_blob = Blob(len(data), ctypes.cast(source, ctypes.POINTER(ctypes.c_ubyte)))
    output_blob = Blob()
    crypt = ctypes.WinDLL("crypt32", use_last_error=True)
    crypt.CryptUnprotectData.argtypes = [ctypes.POINTER(Blob), ctypes.c_void_p, ctypes.c_void_p,
                                         ctypes.c_void_p, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(Blob)]
    crypt.CryptUnprotectData.restype = wintypes.BOOL
    if not crypt.CryptUnprotectData(ctypes.byref(input_blob), None, None, None, None, 1, ctypes.byref(output_blob)):
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        return ctypes.string_at(output_blob.pbData, output_blob.cbData).decode()
    finally:
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.LocalFree.argtypes = [ctypes.c_void_p]
        kernel.LocalFree.restype = ctypes.c_void_p
        kernel.LocalFree(output_blob.pbData)


@unittest.skipUnless(BINARY.is_file(), "Requires a built local Rust braintrade-core executable")
class CoreFullExitTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="braintrade-full-exit-")
        self.root = Path(self.temporary.name)
        self.core = None
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        self.key = ""

    def tearDown(self):
        if self.core is not None and self.core.poll() is None:
            self.core.kill()
            self.core.wait(timeout=5)
        self.temporary.cleanup()

    def boot(self, extra):
        env = dict(os.environ, BRAINTRADE_DATA_DIR=str(self.root), BRAINTRADE_NEWS_OFFLINE="1", **extra)
        self.core = subprocess.Popen([str(BINARY)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if self.core.poll() is not None:
                self.fail(f"Core exited early: {self.core.returncode}")
            try:
                self.key = read_ipc_key(self.root / "ipc.key")
                status = self.api("/v1/status")
                self.assertEqual(status["core_process"]["pid"], self.core.pid)
                return status
            except (OSError, ValueError, urllib.error.URLError):
                time.sleep(0.1)
        self.fail("Core did not become ready")

    def api(self, path, body=None):
        req = urllib.request.Request(
            "http://127.0.0.1:18741" + path,
            data=None if body is None else json.dumps(body).encode(),
            headers={"Authorization": "Bearer " + self.key, "Content-Type": "application/json"},
        )
        with self.opener.open(req, timeout=25) as response:
            return json.load(response)

    def rejected(self, instance):
        with self.assertRaises(urllib.error.HTTPError) as error:
            self.api("/v1/shutdown", {"expected_core_instance": instance})
        self.assertEqual(error.exception.code, 400)
        return json.loads(error.exception.read())["error"]

    def test_real_core_waits_for_gateway_cleanup_and_rejects_unsafe_or_wrong_session(self):
        script = self.root / "gateway_fixture.py"
        script.write_text(
            "import json, os, pathlib, sys\n"
            f"sys.path.insert(0, {str(ROOT / 'gateway')!r})\n"
            "from mt5_gateway import lock_gateway\n"
            "root=pathlib.Path(os.environ['BRAINTRADE_DATA_DIR'])\n"
            "lock=lock_gateway(root)\n"
            "(root/'fixture-ready.json').write_text(json.dumps({'pid':os.getpid()}))\n"
            "sys.stdin.buffer.read()\n"
            "(root/'fixture-cleaned').write_text('clean')\n"
            "lock.close()\n",
            encoding="utf-8",
        )
        status = self.boot({"BRAINTRADE_GATEWAY_SCRIPT": str(script), "BRAINTRADE_PYTHON": sys.executable})
        instance = status["core_process"]["instance_id"]
        deadline = time.monotonic() + 10
        while not (self.root / "fixture-ready.json").exists() and time.monotonic() < deadline:
            time.sleep(0.05)
        self.assertTrue((self.root / "fixture-ready.json").exists(), "fixture gateway failed to start")
        self.rejected("wrong-instance")
        self.api("/v1/start", {})
        self.rejected(instance)
        self.assertIsNone(self.core.poll())
        self.assertFalse((self.root / "fixture-cleaned").exists())
        self.api("/v1/stop", {})
        result = self.api("/v1/shutdown", {"expected_core_instance": instance})
        self.assertTrue(result["gateway_stopped"])
        self.assertEqual(result["core_process"], status["core_process"])
        self.assertTrue((self.root / "fixture-cleaned").exists(), "acknowledged before gateway cleanup")
        self.core.wait(timeout=10)
        self.assertEqual(self.core.returncode, 0)
        # Successful lock acquisition after Core waited for its child ensures
        # the fixture no longer owns the shared gateway lock.
        with lock_gateway(self.root):
            pass

    def test_untracked_gateway_lock_prevents_false_full_exit(self):
        status = self.boot({"BRAINTRADE_GATEWAY_DISABLED": "1"})
        instance = status["core_process"]["instance_id"]
        with lock_gateway(self.root):
            self.rejected(instance)
            self.assertIsNone(self.core.poll())
        result = self.api("/v1/shutdown", {"expected_core_instance": instance})
        self.assertTrue(result["gateway_stopped"])
        self.core.wait(timeout=10)
        self.assertEqual(self.core.returncode, 0)


if __name__ == "__main__":
    unittest.main()
