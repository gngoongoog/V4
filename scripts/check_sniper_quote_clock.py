"""Actual Core + actual Gateway payload, synthetic quotes, NO broker/START."""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / 'gateway'), str(ROOT / 'scripts')]
from mt5_gateway import empty_snapshot, advance_quote_clock, pulse_payload


def check(exe=None):
    with socket.socket() as probe:
        if probe.connect_ex(('127.0.0.1', 18741)) == 0:
            raise RuntimeError('Existing Core detected; refusing to touch it')
    binary = Path(exe or ROOT / 'target/debug/braintrade-core').resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix='v25-quote-clock-') as folder:
        directory = Path(folder)
        env = dict(os.environ, BRAINTRADE_DATA_DIR=folder,
                   BRAINTRADE_GATEWAY_DISABLED='1', BRAINTRADE_NEWS_OFFLINE='1')
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        key = ''

        def api(path, data=None, method=None):
            req = urllib.request.Request('http://127.0.0.1:18741'+path,
                data=None if data is None else json.dumps(data, allow_nan=False).encode(),
                headers={'Authorization':'Bearer '+key,'Content-Type':'application/json'}, method=method)
            with opener.open(req, timeout=8) as r:
                return json.load(r)

        with (directory/'core.log').open('wb') as log:
            process = subprocess.Popen([str(binary)] + (['--core'] if exe else []),
                cwd=folder, env=env, stdout=log, stderr=subprocess.STDOUT)
            try:
                for _ in range(150):
                    if process.poll() is not None:
                        raise RuntimeError('Isolated Core exited before readiness')
                    try:
                        if os.name == 'nt':
                            from check_windows_portable import unseal
                            key = unseal(directory/'ipc.key')
                        else:
                            key = (directory/'ipc.key').read_text()
                        if api('/v1/status')['state'] == 'stopped':
                            break
                    except OSError:
                        time.sleep(.1)
                else:
                    raise TimeoutError('Core not ready')
                config = api('/v1/config')['config']
                config['trading'].update(mode='sniper', sniper_brain='learner')
                api('/v1/trading/settings', dict(trading=config['trading'], commission_per_lot=7.))
                now = int(time.time()*1000)
                state = empty_snapshot('')
                state.update(connected=True, demo=True, account_trade_mode=0,
                    login=17, server='Synthetic-Demo', currency='USD', symbol='XAUUSD',
                    quote_time_ms=now, source_quote_time_ms=now, tick_age_ms=0,
                    bid=2500., ask=2500.2, balance=100., equity=100.)
                api('/v1/gateway/snapshot', state)
                # Logical key well ahead of wall time, but real source is fresh.
                state['quote_time_ms'] = now + 5000
                for price in (2500.1, 2500.2, 2500.3):
                    state, changed = advance_quote_clock(state, dict(state, bid=price, ask=price+.2))
                    assert changed
                    work = api('/v1/execution/pulse', pulse_payload(state), 'PUT')
                    assert work['pulse_fast_path'] is True
                    assert work['command'] is None, 'STOPPED must not emit entry orders'
                cases = [pulse_payload(state)]  # exact duplicate
                old = dict(pulse_payload(state), quote_time_ms=now+6000, source_quote_time_ms=now-1)
                cases.append(old)  # rollback, despite a fresh logical key
                cases.append(dict(old, source_quote_time_ms=now-10_000))
                missing = dict(pulse_payload(state), quote_time_ms=now+6000)
                missing.pop('source_quote_time_ms')
                cases.append(missing)
                for bad in cases:
                    try:
                        api('/v1/execution/pulse', bad, 'PUT')
                    except urllib.error.HTTPError as error:
                        assert error.code == 400
                    else:
                        raise AssertionError('Invalid/duplicate quote was admitted')
                assert api('/v1/status')['requested_run'] is False
                assert api('/v1/execution/gateway')['command'] is None
                assert api('/v1/shutdown', {})['ok'] is True
                assert process.wait(timeout=15) == 0
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=10)
    print(json.dumps(dict(status='PASS', accepted_dense_events=3, rejected_cases=4,
        synthetic_quotes=True, real_core=True, broker_demo='NOT TESTED', no_start=True)))


if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--exe')
    check(parser.parse_args().exe)
