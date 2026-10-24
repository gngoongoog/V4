"""Safely exercise the REAL Rust Core -> Gateway process env handshake.

Regression: a literal '\\n' inserted into a Rust // comment turned
BRAINTRADE_DATA_DIR and BRAINTRADE_IPC_TOKEN assignments into a comment.
Core stayed ONLINE, but Python Gateway quit, leaving MT5 Bridge OFFLINE.

Uses a harmless temporary Python child process, NOT MetaTrader5 or a broker.
The real Rust Core is launched in its own temporary data directory.
"""
from __future__ import annotations
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

ROOT=Path(__file__).resolve().parents[1]
BINARY=ROOT/"target"/"debug"/("braintrade-core.exe" if os.name=="nt" else "braintrade-core")

def main() -> None:
    if os.name=="nt":
        raise SystemExit("Gateway process-env smoke runs on Linux isolated Core CI")
    if not BINARY.is_file():
        raise FileNotFoundError(f"Build native Core first: {BINARY}")
    with tempfile.TemporaryDirectory(prefix="braintrade-gateway-env-") as temporary:
        base=Path(temporary)
        ready=base/"gateway-env-valid.json"
        child_script=base/"gateway_probe.py"
        child_script.write_text(
            "import json,os,pathlib,time\n"
            "root=pathlib.Path(os.environ['BRAINTRADE_DATA_DIR']).resolve()\n"
            "token=os.environ['BRAINTRADE_IPC_TOKEN']\n"
            "assert root.is_dir() and len(token)>16\n"
            "assert os.environ.get('PYTHONDONTWRITEBYTECODE')=='1'\n"
            "assert (root/'ipc.key').is_file()\n"
            "pathlib.Path(os.environ['GATEWAY_TEST_RESULT']).write_text(json.dumps({"
            "'data_dir_ok':True,'token_present':True,'disable_bytecode':True}))\n"
            "while True: time.sleep(1)\n",
            encoding="utf-8",
        )
        env=dict(os.environ)
        env.pop("BRAINTRADE_GATEWAY_DISABLED",None)
        env.update({
            "BRAINTRADE_DATA_DIR":str(base),
            "BRAINTRADE_GATEWAY_SCRIPT":str(child_script),
            "BRAINTRADE_PYTHON":sys.executable,
            "BRAINTRADE_NEWS_OFFLINE":"1",
            "GATEWAY_TEST_RESULT":str(ready),
        })
        log=base/"core-stdout.log"
        with log.open("wb") as handle:
            core=subprocess.Popen([str(BINARY)],env=env,
                                  stdout=handle,stderr=subprocess.STDOUT)
            try:
                for _ in range(180):
                    if ready.is_file():
                        value=json.loads(ready.read_text(encoding="utf-8"))
                        assert value=={
                            "data_dir_ok":True,
                            "token_present":True,
                            "disable_bytecode":True,
                        },value
                        assert core.poll() is None,"Core unexpectedly exited"
                        print("PASS: Core spawned Gateway with IPC token, data directory and no bytecode")
                        return
                    if core.poll() is not None:
                        raise RuntimeError(
                            f"Core quit {core.returncode}; logs: "
                            +log.read_text(errors="replace")[-1500:])
                    time.sleep(0.1)
                raise AssertionError(
                    "Core never spawned correctly credentialed Gateway; logs: "
                    +log.read_text(errors="replace")[-1500:])
            finally:
                if core.poll() is None:
                    core.terminate()
                    try:core.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        core.kill();core.wait(timeout=5)

if __name__=="__main__":
    main()
