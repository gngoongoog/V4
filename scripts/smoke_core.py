"""Integration checks against a real Rust core using explicit test fixtures.
No MT5 connection or trade submission. Run with the normal application closed.
"""
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import time
import urllib.request
import urllib.error

ROOT=Path(__file__).resolve().parents[1]
BINARY=ROOT/'target'/'debug'/('braintrade-core.exe' if os.name=='nt' else 'braintrade-core')
BASE='http://127.0.0.1:18741'

def main():
    if os.name=='nt':
        raise SystemExit('This fixture harness currently uses the Linux development IPC key. Run native unit tests on Windows.')
    with tempfile.TemporaryDirectory(prefix='braintrade-smoke-') as directory:
        env=dict(os.environ,BRAINTRADE_DATA_DIR=directory,BRAINTRADE_NEWS_OFFLINE='1')
        env.pop('BRAINTRADE_GATEWAY_SCRIPT',None)
        process=subprocess.Popen([str(BINARY)],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        key=''
        opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
        def api(path,body=None,auth=True,origin=False):
            headers={'Content-Type':'application/json'}
            if auth:headers['Authorization']='Bearer '+key
            if origin:headers['Origin']='https://untrusted.invalid'
            req=urllib.request.Request(BASE+path,data=json.dumps(body).encode() if body is not None else None,headers=headers)
            with opener.open(req,timeout=3) as response:return json.load(response)
        try:
            for _ in range(80):
                try:
                    key=(Path(directory)/'ipc.key').read_text()
                    status=api('/v1/status');break
                except (OSError,urllib.error.URLError):time.sleep(.1)
            else:raise AssertionError('Core startup failed')
            assert not status['execution_enabled'] and status['state']=='stopped'
            news=api('/v1/news');assert news['offline'] and len(news['sources'])==3 and news['items']==[]
            assert all(s['stale'] for s in news['sources'])
            api('/v1/news/settings',{'enabled':False});assert not api('/v1/news')['enabled']
            try:api('/v1/news/refresh',{})
            except urllib.error.HTTPError as e:assert e.code==400
            else:raise AssertionError('Offline news refresh should be rejected')
            for kwargs in ({'auth':False},{'origin':True}):
                try:api('/v1/status',**kwargs)
                except urllib.error.HTTPError as e:assert e.code==401
                else:raise AssertionError('Unauthorized request accepted')
            second=subprocess.run([str(BINARY)],env=env,timeout=5,capture_output=True)
            assert second.returncode==0,'Second core should exit cleanly'
            assert api('/v1/start',{})['state']=='starting'
            import sys
            sys.path.insert(0,str(ROOT/'gateway'))
            from mt5_gateway import empty_snapshot
            snapshot=empty_snapshot('TEST FIXTURE')
            snapshot.update(connected=True,login=123,currency='USD',balance=1000.0,equity=1005.0,server='TEST ONLY')
            api('/v1/gateway/snapshot',snapshot)
            assert api('/v1/status')['state']=='monitoring'
            # Stale snapshots must withdraw current account state availability.
            time.sleep(8.3)
            assert api('/v1/status')['state']=='degraded'
            config=api('/v1/config')['config']
            invalid=dict(config,telegram_enabled=True)
            try:api('/v1/config',{'config':invalid})
            except urllib.error.HTTPError as e:assert e.code==400
            else:raise AssertionError('Invalid Telegram config accepted')
            api('/v1/config',{'config':config})
            assert not api('/v1/status')['gateway_fresh']
            now=int(time.time()*1000)
            def bars(seconds):
                boundary=now//1000//seconds*seconds
                return [dict(time=boundary-(180-i)*seconds,open=2000+i*.3,high=2000+i*.3+.6,low=2000+i*.3-.6,close=2000+i*.3+.25,tick_volume=100) for i in range(180)]
            snapshot.update(symbol='XAUUSD',equity=10000.,balance=10000.,bid=2054.,ask=2054.1,tick_age_ms=0,tick_size=.01,tick_value_loss=1.,tick_value_profit=1.,volume_min=.01,volume_step=.01,volume_max=100.,market=dict(captured_ms=now,m1=bars(60),m5=bars(300),m15=bars(900),error=''))
            api('/v1/gateway/snapshot',snapshot)
            shadow=api('/v1/status');assert shadow['analysis']['action']=='SHADOW_BUY',shadow['analysis'];assert not shadow['execution_enabled']
            target=shadow['analysis']['target'];api('/v1/gateway/snapshot',snapshot)
            assert len(api('/v1/decisions')['decisions'])==1
            risk=config['risk'];risk['risk_percent']=.35;api('/v1/risk',risk)
            assert api('/v1/config')['config']['risk']['risk_percent']==.35
            try:api('/v1/risk',dict(risk,max_lots=0))
            except urllib.error.HTTPError as e:assert e.code==400
            else:raise AssertionError('Invalid risk settings accepted')
            assert api('/v1/stop',{})['state']=='stopped'
            snapshot.update(bid=target+.1,ask=target+.2);api('/v1/gateway/snapshot',snapshot)
            paper=api('/v1/status')['paper'];assert paper['trades']==1 and paper['realized']>0
            assert isinstance(api('/v1/opportunities/history')['setups'], list)
            export=Path(api('/v1/export',{})['path'])
            assert (export/'opportunity_setups.jsonl').exists()
            assert len((export/'decisions.jsonl').read_text().splitlines())==1
            assert len((export/'paper_events.jsonl').read_text().splitlines())==2
            assert len((export/'candles.jsonl').read_text().splitlines())==540

            from check_backtest import check
            backtest_id,backtest_metrics,candidate_id,comparison=check(api,snapshot)

            # Full public-API chain: independent chronological evidence -> challenger
            # registration -> immutable review prerequisite -> read-only prediction.
            from check_learning import check as check_learning
            learning_ids=check_learning(api,snapshot)
            assert len(api('/v1/backtest/runs')['runs'])==8
            assert len(api('/v1/evaluation/runs')['runs'])==1
            assert len(api('/v1/learning/review/records')['records'])==1
            assert len(api('/v1/learning/inference/recent')['predictions'])==1
            assert not api('/v1/status')['execution_enabled']

            backup=api('/v1/backup',{})
            folder=Path(backup['path'])
            assert (folder/'config.json').exists()
            with sqlite3.connect(folder/'events.sqlite') as db:
                assert db.execute('PRAGMA integrity_check').fetchone()[0]=='ok'
                assert db.execute('SELECT COUNT(*) FROM events').fetchone()[0]>3
                assert db.execute('SELECT COUNT(*) FROM chronological_evaluations').fetchone()[0]==1
                assert db.execute('SELECT COUNT(*) FROM learning_review_records').fetchone()[0]==1
                assert db.execute('SELECT COUNT(*) FROM learning_predictions').fetchone()[0]==1
                assert db.execute("SELECT COUNT(*) FROM learning_slots WHERE slot='champion' AND model_id IS NOT NULL").fetchone()[0]==0
            assert not list(folder.glob('*.sealed'))
            events_before=api('/v1/events')['events']
            api('/v1/shutdown',{});process.wait(timeout=5)
            assert process.returncode==0
            process=subprocess.Popen([str(BINARY)],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            for _ in range(50):
                try:status=api('/v1/status');break
                except OSError:time.sleep(.1)
            assert status['state']=='stopped' and not status['execution_enabled']
            api('/v1/gateway/snapshot',snapshot)
            assert api('/v1/status')['paper']['trades']==1
            assert len(api('/v1/decisions')['decisions'])==1
            assert api('/v1/config')['config']['risk']['risk_percent']==.35
            assert len(api('/v1/backtest/runs')['runs'])==8
            assert api('/v1/backtest/report',{'id':candidate_id})['comparison']==comparison
            assert not api('/v1/news')['enabled']
            assert api('/v1/backtest/report',{'id':backtest_id})['metrics']==backtest_metrics
            assert api('/v1/evaluation/report',{'id':learning_ids['evaluation_id']})['immutable'] is True
            reviews=api('/v1/learning/review/records')['records']
            assert reviews[0]['id']==learning_ids['review_id']
            inference=api('/v1/learning/inference')
            assert inference['predictions']==1 and inference['shadow_effect'] is False
            assert inference['broker_effect'] is False
            assert len(api('/v1/events')['events'])>len(events_before)
            api('/v1/shutdown',{});process.wait(timeout=5)
            assert process.returncode==0
            print('PASS: lifecycle/auth/staleness/shadow/risk; backtest UTC chunking, cancel, account isolation, asynchronous completion, full export, deterministic offline replay, CES/Meta-v2 experiments, chronological evidence, challenger review eligibility, cross-language feature parity, read-only inference persistence/isolation, news API/settings, backup and restart persistence')
        finally:
            if process.poll() is None:process.terminate();process.wait(timeout=5)

if __name__=='__main__':main()
