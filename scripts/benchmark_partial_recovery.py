"""Compare unchanged recovery polls using a fake broker and local SQLite.

Not a Windows, EA, network, broker-fill or live-trading benchmark.
The baseline is the same current source with only the no-op-write guard removed.
"""
from __future__ import annotations
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import time
from types import SimpleNamespace

root = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(root/'gateway'), str(root/'tests')]
from test_broker_execution import Broker, command

source = (root/'gateway/execution.py').read_text()
begin = source.index('            # The same partial history can be polled repeatedly')
end = source.index('            return self._save(cmd, result)', begin)
old_source = source[:begin] + source[end:]

def measure(code, db, n=400):
    ns = {'__name__': 'bench_execution', '__file__': str(root/'gateway/execution.py')}
    exec(compile(code, str(root/'gateway/execution.py'), 'exec'), ns)
    executor = ns['BrokerExecutor'](db)
    broker = Broker()
    cmd = command()
    broker.raise_send = True
    executor.execute(broker, cmd, lambda _: True)
    broker.raise_send = False
    broker.deals = [SimpleNamespace(ticket=200, order=0, magic=cmd['magic'],
        symbol=cmd['symbol'], comment='BT:'+cmd['id'][:20], volume=.04,
        type=0, entry=0, position_id=700, price=2000., time_msc=1000)]
    first = executor.execute(broker, cmd, lambda _: True)
    assert first['state'] == 'partial'
    writes = []
    executor.db.set_trace_callback(lambda s: writes.append(s) if s.lstrip().upper().startswith('UPDATE SENDS') else None)
    samples = []
    for _ in range(n):
        started = time.perf_counter_ns()
        result = executor.execute(broker, cmd, lambda _: True)
        samples.append((time.perf_counter_ns()-started)/1e6)
        assert result['state'] == 'partial'
    executor.db.set_trace_callback(None)
    executor.close()
    assert len(broker.sends) == 1
    samples.sort()
    pct = lambda p: round(samples[round((len(samples)-1)*p)], 4)
    return {'samples':n,'p50_ms':pct(.5),'p95_ms':pct(.95),'p99_ms':pct(.99),'sqlite_updates':len(writes)}

with tempfile.TemporaryDirectory() as temp:
    baseline = measure(old_source, Path(temp)/'old.sqlite')
    updated = measure(source, Path(temp)/'new.sqlite')
    print(json.dumps({'scope':'local Python fake MT5 unchanged partial recovery; not Windows or broker latency',
                      'baseline':baseline,'updated':updated}, ensure_ascii=False))
