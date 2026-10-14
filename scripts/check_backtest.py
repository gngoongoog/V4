"""Real-core backtest integration using explicitly synthetic historical fixtures."""
import csv
import hashlib
from datetime import datetime, timezone
import json
from pathlib import Path
import shutil
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
    detailed={
        'fills.csv':'trade_index,fill_index,side,entry_time_utc,fill_time_utc,phase,price,lots,commission,net,reason,ambiguous',
        'stop_updates.csv':'trade_index,update_index,side,entry_time_utc,effective_time_utc,price,reason',
        'targets.csv':'trade_index,target_index,side,entry_time_utc,price',
    }
    for name,header in detailed.items():
        path=folder/name
        assert path.is_file(),f'Missing detailed ledger export: {name}'
        assert path.read_text().splitlines()[0]==header
    html=(folder/'report.html').read_text()
    assert '<svg' in html and 'fills.csv' in html and 'stop_updates.csv' in html and 'targets.csv' in html
    assert len(json.loads((folder/'history.json').read_text())['bars'])==9*1440
    from mt5_gateway import empty_snapshot
    api('/v1/gateway/snapshot',empty_snapshot('OFFLINE FIXTURE'))
    api('/v1/backtest/replay',dict(id=first_id))
    second_id=completed();assert second_id!=first_id
    second_folder=Path(api('/v1/backtest/export',dict(id=second_id))['path'])
    assert json.loads((second_folder/'report.json').read_text())==report, 'Offline replay must be deterministic'
    for name in detailed:
        assert (second_folder/name).read_bytes()==(folder/name).read_bytes(),f'Offline replay ledger export differs: {name}'
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
    # Exercise a real KTE export with a known impulse/retrace and three fills.
    api('/v1/gateway/snapshot',snapshot)
    api('/v1/backtest/start',dict(request,strategy='kte_v1'))
    fixture_day=int(datetime(2024,10,1,tzinfo=timezone.utc).timestamp())
    while True:
        job=api('/v1/gateway/history/request')['job']
        if job is None:break
        bars=[]
        if job['from']==fixture_day:
            for i in range(120):
                p=100+i*.02
                values=(p,p+.12,p-.08,p+.02) if i<100 else (102,103.05,101.95,103) if i==100 else (102.85,103,102.7,102.8) if i<105 else (102.65,102.85,102.4,102.75)
                values={106:(102.75,102.95,102.7,102.85),107:(102.85,103.4,102.8,103.3),108:(103.3,104,103.25,103.9),109:(103.9,104.8,103.8,104.5)}.get(i,values)
                o,h,l,c=values
                bars.append(dict(time=job['from']+i*60,open=o,high=h,low=l,close=c,tick_volume=100,spread_points=2))
        api('/v1/gateway/history/chunk',dict(job,bars=bars,error=''))
    kte_id=completed()
    kte_folder=Path(api('/v1/backtest/export',dict(id=kte_id))['path'])
    kte=json.loads((kte_folder/'report.json').read_text())
    assert kte['algorithm'].startswith('kte-v1')
    assert len(kte['trades'])==1,kte['metrics']
    trade=kte['trades'][0]
    assert [f['reason'] for f in trade['fills']]==['tp1','tp2','tp3']
    assert len(trade['stop_updates'])>=2
    assert abs(sum(f['net'] for f in trade['fills'])-kte['metrics']['net'])<1e-8
    assert len(list(csv.DictReader((kte_folder/'fills.csv').open())))==3
    assert len(list(csv.DictReader((kte_folder/'stop_updates.csv').open())))==len(trade['stop_updates'])
    assert 'trade-select' in (kte_folder/'report.html').read_text()
    # These exports contain only explicit synthetic fixtures, never broker data.
    review=Path(__file__).resolve().parents[1]/'test-output'
    for source,name in [(kte_folder,'kte-report'),(third_folder,'empty-report'),(folder,'meta-report')]:
        shutil.copytree(source,review/name,dirs_exist_ok=True)
    api('/v1/gateway/snapshot',empty_snapshot('OFFLINE KTE FIXTURE'))
    api('/v1/backtest/replay',dict(id=kte_id))
    replay_id=completed()
    replay_folder=Path(api('/v1/backtest/export',dict(id=replay_id))['path'])
    assert json.loads((replay_folder/'report.json').read_text())==kte
    for name in detailed:
        assert (replay_folder/name).read_bytes()==(kte_folder/name).read_bytes()
    assert (replay_folder/'report.html').read_bytes()==(kte_folder/'report.html').read_bytes()
    assert len(api('/v1/backtest/runs')['runs'])==5
    # Guard the complete legacy report, not only a handful of totals. Digests come
    # from verified pre-Meta-v2 CI synthetic artifacts; captured_ms alone varies.
    goldens=json.loads((Path(__file__).resolve().parents[1]/'tests/legacy_report_hashes.json').read_text())
    for old in (report,ces,kte):
        normalized=dict(old);normalized.pop('captured_ms')
        digest=hashlib.sha256(json.dumps(normalized,sort_keys=True,separators=(',',':')).encode()).hexdigest()
        assert digest==goldens['reports'][old['options']['strategy']], (old['algorithm'],digest)
    # MT5 is offline. Experiment must use the original stored history and options.
    api('/v1/backtest/experiment',dict(id=first_id,strategy='meta_v2'))
    candidate_id=completed()
    candidate_folder=Path(api('/v1/backtest/export',dict(id=candidate_id))['path'])
    candidate=json.loads((candidate_folder/'report.json').read_text())
    comparison=json.loads((candidate_folder/'comparison.json').read_text())
    assert candidate['algorithm']=='meta-v2-candidate-v1 / M1-OHLC-conservative-v1'
    assert candidate['metrics']['trades']>0
    assert candidate['options']==dict(report['options'],strategy='meta_v2')
    assert (candidate_folder/'history.json').read_bytes()==(folder/'history.json').read_bytes()
    assert comparison['source_id']==first_id
    assert comparison['identical_history'] and comparison['identical_options_except_strategy']
    assert comparison['source_metrics']==report['metrics'] and comparison['target_metrics']==candidate['metrics']
    assert abs(comparison['delta_net']-(candidate['metrics']['net']-report['metrics']['net']))<1e-8
    candidate_view=api('/v1/backtest/report',dict(id=candidate_id))
    assert candidate_view['comparison']==comparison and 'مقارنة بنفس البيانات' in candidate_view['text']
    decisions=candidate['decisions']
    assert len(decisions)==candidate['metrics']['tested_bars']
    assert sum(d['outcome']=='entered' for d in decisions)==len(candidate['trades'])
    rows=list(csv.DictReader((candidate_folder/'decisions.csv').open()))
    assert len(rows)==len(decisions)
    for d,row in zip(decisions,rows):
        assert int(row['time_utc'])==d['time'] and int(row['signal_bar_utc'])==d['signal_bar']
        assert row['outcome']==d['outcome'] and row['candidate_action']==d['candidate']['action']
        assert row['gates']==' | '.join(d['candidate']['gates'])
    assert 'experiment-comparison' in (candidate_folder/'report.html').read_text()
    assert 'candidate-audit' in (candidate_folder/'report.html').read_text()
    shutil.copytree(candidate_folder,review/'meta-v2-report',dirs_exist_ok=True)
    api('/v1/backtest/replay',dict(id=candidate_id))
    candidate_replay_id=completed()
    replay_folder=Path(api('/v1/backtest/export',dict(id=candidate_replay_id))['path'])
    for name in ('report.json','history.json','decisions.csv','comparison.json','report.html'):
        assert (replay_folder/name).read_bytes()==(candidate_folder/name).read_bytes(),name
    assert len(api('/v1/backtest/runs')['runs'])==7
    api('/v1/gateway/snapshot',snapshot)
    return first_id,view['metrics'],candidate_id,comparison
