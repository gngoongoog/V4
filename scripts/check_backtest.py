"""Real-core backtest integration using explicitly synthetic historical fixtures."""
import csv
import json
from pathlib import Path
import time
import urllib.error

def check(api, snapshot):
    def rejected(path, payload):
        try: api(path,payload)
        except urllib.error.HTTPError as error: assert error.code==400
        else: raise AssertionError('Invalid backtest request accepted: '+path)
    snapshot['point']=.01
    api('/v1/gateway/snapshot',snapshot)
    request=dict(from_date='2024-10-01',to_date='2024-10-03',initial_equity=10000.,spread_floor=.02)
    api('/v1/start',{})
    rejected('/v1/backtest/start',request)
    api('/v1/stop',{})
    rejected('/v1/backtest/start',dict(request,to_date='2024-02-30'))
    rejected('/v1/backtest/start',dict(request,to_date='2099-01-01'))
    api('/v1/backtest/start',request)
    old=api('/v1/gateway/history/request')['job']
    api('/v1/backtest/cancel',{})
    rejected('/v1/gateway/history/chunk',dict(old,bars=[],error=''))
    assert api('/v1/backtest')['state']=='cancelled'
    api('/v1/backtest/start',request)
    rejected('/v1/gateway/history/chunk',dict(old,bars=[],error=''))
    assert api('/v1/backtest')['state']=='loading', 'Late old chunk killed new job'
    rejected('/v1/start',{})
    wrong=api('/v1/gateway/history/request')['job']
    rejected('/v1/gateway/history/chunk',dict(wrong,server='WRONG SERVER',bars=[],error=''))
    assert api('/v1/backtest')['state']=='error'
    assert api('/v1/backtest/runs')['runs']==[]
    api('/v1/backtest/start',request)
    first=api('/v1/gateway/history/request')['job']['from']
    while True:
        job=api('/v1/gateway/history/request')['job']
        if job is None:break
        bars=[]
        for second in range(job['from'],job['to'],60):
            p=2000+(second-first)/60*.1
            bars.append(dict(time=second,open=p,high=p+.3,low=p-.1,close=p+.08,tick_volume=100,spread_points=2))
        api('/v1/gateway/history/chunk',dict(job,bars=bars,error=''))
        assert not api('/v1/status')['execution_enabled']
    def completed():
        deadline=time.monotonic()+20
        while time.monotonic()<deadline:
            state=api('/v1/backtest')
            if state['state']=='completed':return state['latest_id']
            assert state['state']!='error',state
            time.sleep(.025)
        raise AssertionError('Backtest worker timed out')
    first_id=completed()
    view=api('/v1/backtest/report',dict(id=first_id))
    assert view['metrics']['trades']>0,view
    folder=Path(api('/v1/backtest/export',dict(id=first_id))['path'])
    report=json.loads((folder/'report.json').read_text())
    trades=list(csv.DictReader((folder/'trades.csv').open()))
    assert len(trades)==report['metrics']['trades']
    assert abs(sum(float(t['net']) for t in trades)-report['metrics']['net'])<1e-6
    assert '<svg' in (folder/'report.html').read_text()
    assert len(json.loads((folder/'history.json').read_text())['bars'])==9*1440
    from mt5_gateway import empty_snapshot
    api('/v1/gateway/snapshot',empty_snapshot('OFFLINE FIXTURE'))
    api('/v1/backtest/replay',dict(id=first_id))
    second_id=completed();assert second_id!=first_id
    second_folder=Path(api('/v1/backtest/export',dict(id=second_id))['path'])
    assert json.loads((second_folder/'report.json').read_text())==report, 'Offline replay must be deterministic'
    assert len(api('/v1/backtest/runs')['runs'])==2
    rejected('/v1/backtest/experiment',dict(id=first_id,strategy='not_implemented'))
    api('/v1/backtest/experiment',dict(id=first_id,strategy='ces_v1'))
    third_id=completed()
    third_folder=Path(api('/v1/backtest/export',dict(id=third_id))['path'])
    ces=json.loads((third_folder/'report.json').read_text())
    assert ces['algorithm'].startswith('ces-v1') and ces['options']['strategy']=='ces_v1'
    assert ces['options']['risk']==report['options']['risk']
    assert (third_folder/'history.json').read_bytes()==(folder/'history.json').read_bytes()
    assert ces['metrics']['trades']==0, 'Constant entropy must not invent a CES signal'
    assert len(api('/v1/backtest/runs')['runs'])==3
    api('/v1/gateway/snapshot',snapshot)
    return first_id,view['metrics']
