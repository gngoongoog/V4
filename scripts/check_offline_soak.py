"""Bounded real-Core endurance/profile harness, synthetic quotes, NO MT5.

Runs in a fresh temporary data directory with gateway/news disabled and the
execution account left unbound. Never attaches to an existing Core. Results
measure this host's local HTTP/process work, not broker fills or profitability.
"""
from __future__ import annotations

import argparse
from collections import deque
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing, contextmanager
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / 'gateway'), str(ROOT / 'scripts')]
from mt5_gateway import empty_snapshot, pulse_payload
from transport import LoopbackCore

BASE = 'http://127.0.0.1:18741'


@contextmanager
def temporary_case(report, output):
    """A teardown failure must never leave a successful evidence report."""
    try:
        with tempfile.TemporaryDirectory(prefix='braintrade-offline-soak-') as folder:
            yield folder
    except BaseException as error:
        report.update(status='FAIL', error=f'{type(error).__name__}: {error}')
        output.write_text(json.dumps(report, indent=2, ensure_ascii=False)+'\n', encoding='utf-8')
        raise


class Samples:
    """Bound memory even for a day-long run; explicitly report percentile scope."""
    def __init__(self):
        self.values = deque(maxlen=50_000)
        self.count = 0
        self.total = 0.0
        self.maximum = 0.0

    def add(self, value):
        self.values.append(value)
        self.count += 1
        self.total += value
        self.maximum = max(self.maximum, value)

    def report(self):
        ordered = sorted(self.values)
        return dict(count=self.count, percentile_samples=len(ordered),
                    percentile_scope='latest at most 50000 samples',
                    mean_ms=round(self.total / self.count, 3) if self.count else None,
                    max_ms=round(self.maximum, 3) if self.count else None,
                    **{f'p{p}_ms': round(ordered[math.ceil(len(ordered)*p/100)-1], 3)
                       if ordered else None for p in (50, 95, 99)})


def process_usage(pid):
    """Resident memory and total CPU seconds, using OS APIs, no new dependency."""
    if sys.platform.startswith('linux'):
        stat = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
        rss = int(Path(f'/proc/{pid}/statm').read_text().split()[1]) * os.sysconf('SC_PAGE_SIZE')
        cpu = (int(stat[11]) + int(stat[12])) / os.sysconf('SC_CLK_TCK')
        return rss, cpu
    if os.name == 'nt':
        import ctypes as ct
        from ctypes import wintypes as wt
        class Memory(ct.Structure):
            _fields_ = [('cb', wt.DWORD), ('faults', wt.DWORD)] + [
                (n, ct.c_size_t) for n in ('peak', 'rss', 'peak_paged', 'paged',
                                         'peak_nonpaged', 'nonpaged', 'pagefile', 'peak_pagefile')]
        kernel = ct.WinDLL('kernel32', use_last_error=True)
        psapi = ct.WinDLL('psapi', use_last_error=True)
        kernel.OpenProcess.argtypes = [wt.DWORD, wt.BOOL, wt.DWORD]
        kernel.OpenProcess.restype = wt.HANDLE
        kernel.CloseHandle.argtypes = [wt.HANDLE]
        kernel.GetProcessTimes.argtypes = [wt.HANDLE] + [ct.POINTER(wt.FILETIME)] * 4
        psapi.GetProcessMemoryInfo.argtypes = [wt.HANDLE, ct.POINTER(Memory), wt.DWORD]
        handle = kernel.OpenProcess(0x0400 | 0x0010, False, pid)
        if not handle:
            raise ct.WinError(ct.get_last_error())
        try:
            memory = Memory(); memory.cb = ct.sizeof(memory)
            times = [wt.FILETIME() for _ in range(4)]
            if not psapi.GetProcessMemoryInfo(handle, ct.byref(memory), memory.cb):
                raise ct.WinError(ct.get_last_error())
            if not kernel.GetProcessTimes(handle, *[ct.byref(t) for t in times]):
                raise ct.WinError(ct.get_last_error())
            cpu = sum((t.dwHighDateTime << 32) + t.dwLowDateTime for t in times[2:]) / 10_000_000
            return memory.rss, cpu
        finally:
            kernel.CloseHandle(handle)
    raise RuntimeError('Process sampling supports Linux and Windows')


def run(args):
    binary = Path(args.binary).resolve(strict=True)
    # Check before any HTTP call: never shut down or modify a user's Core.
    with socket.socket() as probe:
        if probe.connect_ex(('127.0.0.1', 18741)) == 0:
            raise RuntimeError('Existing Core detected; refusing to touch it')
    output = Path(args.output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    report = dict(status='RUNNING', scope='synthetic localhost; no MT5/broker',
                  broker_execution_tested=False, real_money_ready=False,
                  platform=platform.platform(), python=sys.version.split()[0],
                  binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                  requested_seconds=args.seconds, target_hz=args.hz, phases=[], checks={},
                  transport=args.transport, throughput_asserted=False,
                  persistent_http_sessions=[],
                  bulk_io_transport='separate fresh HTTP connection')
    started = time.monotonic()
    metrics = {n: Samples() for n in ('pulse_http', 'pulse_core', 'snapshot_http', 'status_http')}
    process = None
    with temporary_case(report, output) as folder:
        directory = Path(folder)
        env = dict(os.environ, BRAINTRADE_DATA_DIR=folder,
                   BRAINTRADE_GATEWAY_DISABLED='1', BRAINTRADE_NEWS_OFFLINE='1')
        env.pop('BRAINTRADE_GATEWAY_SCRIPT', None)
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        key = ''
        persistent_client = None
        owner_thread = threading.get_ident()
        log = (directory / 'core.log').open('wb')

        def close_client():
            nonlocal persistent_client
            if persistent_client is not None:
                report['persistent_http_sessions'].append(dict(
                    requests=persistent_client.requests,
                    connections=persistent_client.reconnects))
                persistent_client.close()
                persistent_client = None

        def api(path, body=None, method=None, metric=None, timeout=10):
            nonlocal persistent_client
            before = time.perf_counter()
            if (args.transport == 'persistent' and timeout == 10
                    and threading.get_ident() == owner_thread):
                # Exercise the production Gateway client, including its rule
                # that an ambiguous request is NEVER replayed automatically.
                # Bulk export retains its own connection/thread and timeout.
                if persistent_client is None:
                    persistent_client = LoopbackCore(key, timeout=timeout)
                value = persistent_client.api(path, body, method)
            else:
                request = urllib.request.Request(BASE+path,
                    data=None if body is None else json.dumps(body, allow_nan=False).encode(),
                    headers={'Authorization': 'Bearer '+key, 'Content-Type': 'application/json'},
                    method=method)
                with opener.open(request, timeout=timeout) as response:
                    value = json.load(response)
            if metric:
                metrics[metric].add((time.perf_counter()-before)*1000)
            if path == '/v1/shutdown':
                close_client()
            return value

        def boot():
            nonlocal process, key
            close_client()
            process = subprocess.Popen([str(binary)] + (['--core'] if args.desktop else []),
                cwd=folder, env=env, stdout=log, stderr=subprocess.STDOUT)
            for _ in range(150):
                if process.poll() is not None:
                    raise RuntimeError('Isolated Core exited before readiness')
                try:
                    if os.name == 'nt':
                        from check_windows_portable import unseal
                        key = unseal(directory / 'ipc.key')
                    else:
                        key = (directory / 'ipc.key').read_text()
                    state = api('/v1/status')
                    assert state['state'] == 'stopped' and not state['requested_run']
                    return
                except OSError:
                    time.sleep(.1)
            raise TimeoutError('Isolated Core not ready')

        sequence = 0
        logical_ms = 0

        def quote():
            nonlocal sequence, logical_ms
            sequence += 1
            now = int(time.time()*1000)
            logical_ms = max(logical_ms+1, now)
            price = round(2500 + math.sin(sequence/30)*.4, 5)
            return dict(bid=price, ask=round(price+.1, 5), quote_time_ms=logical_ms,
                        source_quote_time_ms=now, tick_age_ms=0)

        snapshot = empty_snapshot('OFFLINE SYNTHETIC SOAK')
        snapshot.update(connected=True, demo=True, account_trade_mode=0,
            login=17, server='Synthetic-Demo', currency='USD', symbol='XAUUSD',
            balance=100., equity=100., point=.01, tick_size=.01,
            tick_value_loss=1., tick_value_profit=1.,
            volume_min=.01, volume_step=.01, volume_max=100.)

        def publish():
            snapshot.update(quote())
            now = int(time.time()*1000)
            market = dict(captured_ms=now, error='')
            for name, seconds in (('m1',60), ('m5',300), ('m15',900)):
                end = now//1000//seconds*seconds
                market[name] = [dict(time=end-(180-i)*seconds, open=2500.,
                    high=2501., low=2499., close=2500.1, tick_volume=100) for i in range(180)]
            snapshot['market'] = market
            api('/v1/gateway/snapshot', snapshot, metric='snapshot_http')

        def status():
            state = api('/v1/status', metric='status_http')
            assert state['execution_enabled'] is False, 'Synthetic soak armed broker execution'
            assert process.poll() is None, 'Core exited under load'
            return state

        try:
            boot()
            config = api('/v1/config')['config']
            config['trading'].update(mode='sniper', sniper_brain='learner')
            api('/v1/trading/settings', dict(trading=config['trading'], commission_per_lot=7.))
            publish()
            assert api('/v1/execution')['settings']['login'] == 0
            # Learning and optional readonly IO share the real Core with pulses.
            for frozen in (False, True):
                api('/v1/learning/control', {'frozen': frozen})
                api('/v1/start', {})
                phase = dict(name='learning_frozen' if frozen else 'learning_active', resources=[])
                begin = time.monotonic()
                rss_start, cpu_start = process_usage(process.pid)
                phase['rss_start_mb'] = round(rss_start/2**20, 3)
                count_start = metrics['pulse_http'].count
                due = begin
                next_snapshot = next_status = next_sample = begin
                end = begin + args.seconds/2
                with ThreadPoolExecutor(max_workers=1) as pool:
                    io_future = None
                    expected_run = True
                    while time.monotonic() < end:
                        now = time.monotonic()
                        if now >= next_snapshot:
                            publish(); next_snapshot = time.monotonic()+.5
                        snapshot.update(quote())
                        work = api('/v1/execution/pulse', pulse_payload(snapshot), 'PUT', 'pulse_http')
                        assert work['pulse_fast_path'] is True and work['command'] is None
                        metrics['pulse_core'].add(work['pulse_to_work_us']/1000)
                        if now >= next_status:
                            assert status()['requested_run'] is expected_run
                            next_status = time.monotonic()+1
                        if now >= next_sample:
                            rss, cpu = process_usage(process.pid)
                            phase['resources'].append(dict(elapsed_s=round(now-begin, 3),
                                rss_mb=round(rss/2**20, 3), cpu_seconds=round(cpu-cpu_start, 3)))
                            next_sample = time.monotonic()+5
                        if io_future is None and now-begin > args.seconds/4:
                            # Bulk IO intentionally requires STOP. Keep feeding
                            # quotes while its readonly worker owns the archive.
                            api('/v1/stop', {})
                            expected_run = False
                            io_future = pool.submit(api, '/v1/export', {}, timeout=60)
                        if io_future is not None and io_future.done() and not expected_run:
                            export = io_future.result()
                            assert Path(export['path']).is_dir()
                            api('/v1/start', {})
                            expected_run = True
                        due += 1/args.hz
                        # Never catch up with a synthetic burst after a slow request.
                        due = max(due, time.monotonic())
                        time.sleep(max(0, due-time.monotonic()))
                    if io_future is not None:
                        export = io_future.result(timeout=65)
                        assert Path(export['path']).is_dir()
                        phase['concurrent_export'] = 'PASS'
                elapsed = time.monotonic()-begin
                rss, cpu = process_usage(process.pid)
                phase.update(seconds=round(elapsed, 3), pulses=metrics['pulse_http'].count-count_start,
                    rss_end_mb=round(rss/2**20, 3), rss_delta_mb=round((rss-rss_start)/2**20, 3),
                    cpu_one_core_percent=round(100*(cpu-cpu_start)/elapsed, 2))
                phase['achieved_hz'] = round(phase['pulses']/elapsed, 2)
                phase['achieved_percent_of_target'] = round(100*phase['pulses']/elapsed/args.hz, 2)
                report['phases'].append(phase)
                api('/v1/stop', {})
                assert not status()['requested_run']

            # No feed for > 8 seconds must become degraded, never armed for MT5.
            publish(); api('/v1/start', {})
            time.sleep(8.5)
            assert status()['state'] == 'degraded'
            report['checks']['feed_stall_degrades'] = 'PASS'
            api('/v1/stop', {})
            publish()
            duplicate = pulse_payload(snapshot)
            try:
                api('/v1/execution/pulse', duplicate, 'PUT')
            except urllib.error.HTTPError as error:
                assert error.code == 400
            else:
                raise AssertionError('Duplicate quote accepted')
            report['checks']['duplicate_quote_rejected'] = 'PASS'
            backup = api('/v1/backup', {}, timeout=60)
            assert backup
            api('/v1/shutdown', {})
            assert process.wait(timeout=15) == 0
            boot()
            publish()
            assert not status()['requested_run']
            assert api('/v1/execution/gateway')['command'] is None
            report['checks']['restart_stays_stopped_no_command'] = 'PASS'
            api('/v1/shutdown', {})
            assert process.wait(timeout=15) == 0
            with socket.socket() as probe:
                assert probe.connect_ex(('127.0.0.1', 18741)) != 0
            report['checks']['shutdown_releases_port'] = 'PASS'
            databases = []
            for db_path in directory.rglob('*.sqlite*'):
                if db_path.name.endswith(('-wal', '-shm')):
                    continue
                # Connection.__exit__ commits/rolls back; it does NOT close.
                # Windows cannot remove the temporary DB while this handle lives.
                with closing(sqlite3.connect(db_path.as_uri()+'?mode=ro', uri=True)) as db:
                    assert db.execute('PRAGMA quick_check').fetchone() == ('ok',)
                databases.append(dict(name=str(db_path.relative_to(directory)), bytes=db_path.stat().st_size))
            assert databases, 'No database checked'
            report['databases'] = databases
            report['checks']['sqlite_integrity'] = 'PASS'
            report['status'] = 'CHECKS_PASSED_CLEANUP_PENDING'
        except Exception as error:
            report['status'] = 'FAIL'
            report['error'] = f'{type(error).__name__}: {error}'
            raise
        finally:
            close_client()
            if process is not None and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill(); process.wait(timeout=5)
            log.close()
            report['elapsed_seconds'] = round(time.monotonic()-started, 3)
            report['metrics'] = {name: value.report() for name, value in metrics.items()}
            output.write_text(json.dumps(report, indent=2, ensure_ascii=False)+'\n', encoding='utf-8')
            if report['status'] == 'FAIL':
                output.with_suffix('.core.log').write_bytes((directory/'core.log').read_bytes())
    report['checks']['temporary_cleanup'] = 'PASS'
    report['status'] = 'PASS'
    report['elapsed_seconds'] = round(time.monotonic()-started, 3)
    output.write_text(json.dumps(report, indent=2, ensure_ascii=False)+'\n', encoding='utf-8')
    print(json.dumps(dict(status=report['status'], output=str(output),
                         pulses=metrics['pulse_http'].count, seconds=report['elapsed_seconds'])))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', default=str(ROOT/'target/debug'/('braintrade-core.exe' if os.name == 'nt' else 'braintrade-core')))
    parser.add_argument('--desktop', action='store_true', help='Pass --core to a desktop executable')
    parser.add_argument('--seconds', type=float, default=180)
    parser.add_argument('--hz', type=float, default=50)
    parser.add_argument('--transport', choices=('fresh', 'persistent'), default='fresh',
                        help='Fresh urllib requests or the actual persistent Gateway HTTP client')
    parser.add_argument('--output', default=str(ROOT/'test-output/offline-soak.json'))
    args = parser.parse_args()
    if not 10 <= args.seconds <= 172800 or not 1 <= args.hz <= 500:
        parser.error('seconds must be 10..172800 and hz must be 1..500')
    run(args)
