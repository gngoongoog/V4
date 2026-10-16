"""Persistent local IPC and isolated read-only MT5 history collection.

Only the parent gateway owns BrokerExecutor. Failed HTTP requests are never replayed.
"""
from __future__ import annotations
from collections import deque
from datetime import datetime, timezone
import http.client
import json
import multiprocessing
import socket
import time


class CoreClient:
    def __init__(self, token, port=18741):
        self.token, self.port, self.connection = token, port, None

    def close(self):
        if self.connection is not None:
            self.connection.close()
        self.connection = None

    def __call__(self, path, payload=None):
        try:
            if self.connection is None:
                self.connection = http.client.HTTPConnection('127.0.0.1', self.port, timeout=4)
                self.connection.connect()
                self.connection.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            body = json.dumps(payload, allow_nan=False, separators=(',', ':')).encode() if payload is not None else None
            self.connection.request('POST' if body is not None else 'GET', path, body=body,
                headers={'Authorization': 'Bearer '+self.token, 'Content-Type': 'application/json'})
            response = self.connection.getresponse()
            data = json.loads(response.read())
            if response.status != 200:
                raise ValueError(data.get('error', 'Core rejected request'))
            return data
        except (OSError, ValueError, http.client.HTTPException) as error:
            self.close()
            if isinstance(error, http.client.HTTPException):
                raise OSError('Local Core connection interrupted') from error
            raise


class LoopMetrics:
    def __init__(self):
        self.samples = deque(maxlen=256)
        self.previous_start = None
        self.last = {}

    def finish(self, started, snapshot_ms, core_ms, execution_ms, target_ms):
        elapsed = (time.perf_counter()-started)*1000
        self.samples.append(elapsed)
        ordered = sorted(self.samples)
        self.last = dict(cycle_ms=elapsed, snapshot_ms=snapshot_ms, core_ms=core_ms,
            execution_ms=execution_ms, target_poll_ms=target_ms,
            interval_ms=(started-self.previous_start)*1000 if self.previous_start is not None else None,
            p50_ms=ordered[int((len(ordered)-1)*.50)], p95_ms=ordered[int((len(ordered)-1)*.95)],
            samples=len(ordered), observed_ms=int(time.time()*1000))
        self.previous_start = started
        return max(.001, target_ms/1000-(time.perf_counter()-started))


class CachedMetadata:
    def __init__(self):
        self.key, self.market, self.summary, self.day, self.received = None, None, (None, None), '', 0.

    def update(self, result):
        self.key = tuple(result['identity'])
        self.market = result['market']
        self.summary = result['net'], result['count']
        self.day, self.received = result['day'], time.monotonic()

    def read(self, mt5, key, now):
        return self.market if tuple(key) == self.key else None

    def daily(self, key, day):
        return self.summary if tuple(key) == self.key and day == self.day and time.monotonic()-self.received <= 10 else (None, None)


def identity(account, symbol):
    return (account.login, account.server, symbol, account.currency) if account else None


def read_job(mt5, job, market_cache):
    from mt5_gateway import collect_history_chunk, day_range, deal_summary
    from studio_feed import collect_frame
    wanted = tuple(job['identity'])
    if identity(mt5.account_info(), wanted[2]) != wanted:
        raise ValueError('Account changed before background read')
    now = datetime.now(timezone.utc)
    kind = job['kind']
    if kind == 'metadata':
        market = market_cache.read(mt5, wanted, now)
        start, end, day = day_range(now)
        net, count = deal_summary(mt5.history_deals_get(start, end), mt5)
        result = dict(identity=wanted, market=market, net=net, count=count, day=day)
    elif kind == 'frame':
        result = collect_frame(mt5, job['job'])
    elif kind == 'history':
        result = collect_history_chunk(mt5, job['job'])
    elif kind == 'correlation':
        from correlation_feed import collect_correlation
        result = collect_correlation(mt5, job['job'])
    elif kind == 'deals':
        start, until = job['from_ms'], job['until_ms']
        rows = mt5.history_deals_get(datetime.fromtimestamp(start/1000, timezone.utc), datetime.fromtimestamp(until/1000, timezone.utc))
        if rows is None:
            raise ValueError('Deal history unavailable')
        fields = ('ticket','position_id','magic','symbol','entry','time_msc','volume','price','profit','commission','swap','fee')
        result = dict(login=wanted[0], server=wanted[1], currency=wanted[3], until_ms=until,
                      deals=[{k:getattr(row,k,0) for k in fields} for row in rows])
    else:
        raise ValueError('Unknown read-only job')
    if identity(mt5.account_info(), wanted[2]) != wanted:
        raise ValueError('Account changed during background read')
    return result


def _worker(pipe, terminal_path):
    import MetaTrader5 as mt5
    from mt5_gateway import MarketCache
    cache, initialized = MarketCache(), False
    try:
        while True:
            job = pipe.recv()
            try:
                if not initialized:
                    initialized = mt5.initialize(terminal_path, timeout=5000) if terminal_path else mt5.initialize(timeout=5000)
                if not initialized:
                    raise ValueError('Background MT5 connection unavailable')
                response = dict(job, result=read_job(mt5, job, cache))
            except Exception as error:
                response = dict(job, error=type(error).__name__+': '+str(error)[:250])
                mt5.shutdown()
                initialized = False
            pipe.send(response)
    except (EOFError, OSError):
        pass
    finally:
        mt5.shutdown()
        pipe.close()


class ReadWorker:
    def __init__(self, terminal_path):
        self.terminal_path = terminal_path
        self.process = self.pipe = self.pending = None
        self.started = 0.

    @property
    def busy(self):
        return self.pending is not None

    def submit(self, job):
        if self.busy:
            return False
        if self.process is None or not self.process.is_alive():
            self.close()
            context = multiprocessing.get_context('spawn')
            self.pipe, child = context.Pipe()
            self.process = context.Process(target=_worker, args=(child, self.terminal_path), daemon=True)
            self.process.start()
            child.close()
        self.pipe.send(job)
        self.pending, self.started = job, time.monotonic()
        return True

    def poll(self):
        if not self.busy:
            return None
        try:
            if self.pipe.poll():
                response = self.pipe.recv()
                self.pending = None
                return response
            if not self.process.is_alive() or time.monotonic()-self.started > 15:
                self.close()
        except (OSError, EOFError):
            self.close()
        return None

    def close(self):
        if self.pipe is not None:
            self.pipe.close()
        if self.process is not None:
            if self.process.is_alive():
                self.process.terminate()
            self.process.join(timeout=.2)
        self.process = self.pipe = self.pending = None
