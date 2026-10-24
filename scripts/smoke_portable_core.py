"""Windows portable Core acceptance: STOPPED, local authenticated read, safe shutdown.

This intentionally does NOT connect to MT5 or place any order. Never run against
an existing user's live Core: use a clean isolated Windows build agent.
Usage: python scripts/smoke_portable_core.py target/portable/.../BrainTradeNext.exe
"""
from __future__ import annotations
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
from check_windows_portable import unseal


def check(executable: Path):
    executable = executable.resolve(strict=True)
    if executable.name.lower() != 'braintradenext.exe':
        raise ValueError('This smoke test only accepts BrainTradeNext.exe')
    if not (executable.parent / 'runtime' / 'gateway' / 'mt5_gateway.py').is_file():
        raise ValueError('Portable gateway missing beside executable')
    if not (executable.parent / 'runtime' / 'python' / 'python.exe').is_file():
        raise ValueError('Portable Python missing beside executable')
    with tempfile.TemporaryDirectory(prefix='braintrade-v22-smoke-') as root:
        home = Path(root)
        log_path = home / 'core.log'
        env = dict(os.environ, BRAINTRADE_DATA_DIR=str(home), BRAINTRADE_GATEWAY_DISABLED='1')
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        with log_path.open('wb') as log:
            process = subprocess.Popen([str(executable), '--core'], cwd=str(executable.parent),
                                       stdout=log, stderr=subprocess.STDOUT, env=env)
            try:
                key = None
                status = None
                for _ in range(100):
                    if process.poll() is not None:
                        raise RuntimeError('Core exited early: ' + log_path.read_text(errors='replace')[-4000:])
                    try:
                        key = unseal(home / 'ipc.key')
                        req = urllib.request.Request('http://127.0.0.1:18741/v1/status',
                                                     headers={'Authorization': 'Bearer ' + key})
                        with opener.open(req, timeout=2) as response:
                            status = json.load(response)
                        if status.get('state') == 'stopped':
                            break
                    except (OSError, ValueError, json.JSONDecodeError):
                        time.sleep(.15)
                else:
                    raise TimeoutError('Isolated Core did not start STOPPED')
                if status.get('requested_run') is not False or status.get('execution_enabled') is not False:
                    raise RuntimeError('Unsafe Core initial state; refusing acceptance')
                req = urllib.request.Request('http://127.0.0.1:18741/v1/shutdown',
                     headers={'Authorization': 'Bearer ' + key, 'Content-Type': 'application/json'},
                     method='POST', data=b'{}')
                with opener.open(req, timeout=5) as response:
                    result = json.load(response)
                if result.get('ok') is not True:
                    raise RuntimeError('Safe shutdown was refused: ' + str(result))
                if process.wait(timeout=20) != 0:
                    raise RuntimeError('Core exited with nonzero status')
                print('PASS portable Core: isolated START=STOPPED, authenticated status, safe exit')
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=10)


if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit('Usage: python scripts/smoke_portable_core.py PATH/BrainTradeNext.exe')
    check(Path(sys.argv[1]))
