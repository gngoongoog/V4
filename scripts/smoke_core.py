"""Integration checks against a real Rust core using explicit test fixtures.
No MT5 connection or trade submission. Run with the normal application closed.
"""
import json
import os
import shutil
from contextlib import closing
from concurrent.futures import ThreadPoolExecutor
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
        def api(path,body=None,auth=True,origin=False,timeout=3):
            headers={'Content-Type':'application/json'}
            if auth:headers['Authorization']='Bearer '+key
            if origin:headers['Origin']='https://untrusted.invalid'
            req=urllib.request.Request(BASE+path,data=json.dumps(body).encode() if body is not None else None,headers=headers)
            with opener.open(req,timeout=timeout) as response:return json.load(response)
        try:
            for _ in range(80):
                try:
                    key=(Path(directory)/'ipc.key').read_text()
                    status=api('/v1/status');break
                except (OSError,urllib.error.URLError):time.sleep(.1)
            else:raise AssertionError('Core startup failed')
            assert not status['execution_enabled'] and status['state']=='stopped'
            # P1-8A real HTTP integration: a large, strictly synthetic
            # research task uses the isolated worker while the Core keeps
            # responding to status requests. No MT5 data or trading orders.
            events=[]
            for i in range(1500):
                oid=f"synthetic-{i}"
                events.extend(({'event':'submit','id':oid,'lots':0.01},
                               {'event':'dispatch','id':oid}))
            research_payload={'account':'synthetic','symbol':'XAUUSD','events':events}
            with ThreadPoolExecutor(max_workers=2) as pool:
                research_future=pool.submit(api,'/v1/research/execution-lifecycle',research_payload)
                concurrent_status=api('/v1/status')
                research_result=research_future.result(timeout=10)
            assert concurrent_status['state']=='stopped'
            assert research_result['status']=='synthetic_only'
            assert research_result['event_count']==len(events)
            assert len(research_result['orders'])==1500
            assert research_result['no_mt5_commands'] is True
            assert research_result['no_broker_reads'] is True
            try:api('/v1/research/execution-lifecycle',{'malformed':'payload'})
            except urllib.error.HTTPError as e:assert e.code==400
            else:raise AssertionError('Invalid offline research request accepted')
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
            snapshot.update(connected=True,login=123,currency='USD',balance=1000.0,equity=1005.0,server='TEST ONLY',demo=True,account_trade_mode=0)
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
            # A rejected config request must be side-effect free: it cannot silently STOP
            # a running/degraded system or cancel its current run state.
            assert api('/v1/status')['state']=='degraded'
            # Broker state is stale: even an otherwise valid config save must
            # fail closed. STOP blocks new entries but does not certify that
            # outstanding live orders/positions can safely be abandoned.
            try:api('/v1/config',{'config':config})
            except urllib.error.HTTPError as e:assert e.code==400
            else:raise AssertionError('Stale broker session allowed config save')
            assert api('/v1/status')['state']=='degraded'
            assert api('/v1/stop',{})['state']=='stopped'
            try:api('/v1/config',{'config':config})
            except urllib.error.HTTPError as e:assert e.code==400
            else:raise AssertionError('STOP incorrectly made stale MT5 state safe')
            # Explicit fresh flat-session evidence is required before changing
            # account/terminal settings. This is a fabricated offline snapshot
            # with no actual broker and no new-order permission.
            snapshot['positions']=[]
            snapshot['pending_orders']=0
            api('/v1/gateway/snapshot',snapshot)
            refreshed=api('/v1/status')
            assert refreshed['gateway_fresh'] and not refreshed['requested_run']
            api('/v1/config',{'config':config})
            assert not api('/v1/status')['gateway_fresh']
            # A successful connection/config save intentionally disarms global START.
            # Re-arming must always be explicit before new virtual or broker entries.
            assert api('/v1/status')['state']=='stopped'
            assert api('/v1/start',{})['state']=='starting'
            now=int(time.time()*1000)
            def bars(seconds):
                boundary=now//1000//seconds*seconds
                return [dict(time=boundary-(180-i)*seconds,open=2000+i*.3,high=2000+i*.3+.6,low=2000+i*.3-.6,close=2000+i*.3+.25,tick_volume=100) for i in range(180)]
            snapshot.update(symbol='XAUUSD',equity=10000.,balance=10000.,bid=2054.,ask=2054.1,tick_age_ms=0,tick_size=.01,tick_value_loss=1.,tick_value_profit=1.,volume_min=.01,volume_step=.01,volume_max=100.,market=dict(captured_ms=now,m1=bars(60),m5=bars(300),m15=bars(900),error=''))
            api('/v1/gateway/snapshot',snapshot)
            shadow=api('/v1/status');assert shadow['analysis']['action']=='SHADOW_BUY',shadow['analysis'];assert not shadow['execution_enabled']
            target=shadow['analysis']['target'];api('/v1/gateway/snapshot',snapshot)
            assert len(api('/v1/decisions')['decisions'])==1
            risk=config['risk'];risk['risk_percent']=.35
            # Execution-affecting risk/cost changes are rejected while START is armed.
            try:api('/v1/risk',risk)
            except urllib.error.HTTPError as e:assert e.code==400
            else:raise AssertionError('Risk settings changed while START was armed')
            assert api('/v1/stop',{})['state']=='stopped'
            api('/v1/risk',risk)
            assert api('/v1/config')['config']['risk']['risk_percent']==.35
            try:api('/v1/risk',dict(risk,max_lots=0))
            except urllib.error.HTTPError as e:assert e.code==400
            else:raise AssertionError('Invalid risk settings accepted')
            # End-to-end HTTP boundary for read-only HVN using a SYNTHETIC
            # flat demo snapshot. There is no real MT5, trading or PnL here.
            hvn_request=api('/v1/hvn/request',{'window_ms':60_000})
            assert hvn_request['queued'] and not hvn_request['trade_authorized']
            hvn_job=api('/v1/gateway/hvn/request')['job']
            assert hvn_job['id']==hvn_request['id']
            assert hvn_job['login']==123 and hvn_job['symbol']=='XAUUSD'
            for path,body in (('/v1/start',{}),('/v1/reconnect',{}),
                              ('/v1/config',{'config':config}),
                              ('/v1/backtest/start',{'from_date':'2024-10-01',
                                  'to_date':'2024-10-03','initial_equity':10000.,
                                  'spread_floor':0.02})):
                try:api(path,body)
                except urllib.error.HTTPError as error:
                    assert error.code==400 and b'HVN' in error.read(),path
                else:raise AssertionError('HVN pending read failed to fence '+path)
            assert api('/v1/hvn')['pending']
            hvn_report={
                'available':True,'read_only':True,'trade_authorized':False,
                'broker_only':True,'exchange_wide':False,
                'source':'mt5_quote_tick_activity','unit':'observed_quote_events',
                'identity':{'server':hvn_job['server'],'login':hvn_job['login'],
                            'symbol':hvn_job['symbol'],'currency':hvn_job['currency']},
                'window_ms':hvn_job['window_ms'],'observations':80,
                'trade_events_observed':0,'quote_events_observed':80,
                'distinct_levels':10,'total_activity':80.0,
                'observed_from_ms':hvn_job['now_ms']-30_000,
                'observed_to_ms':hvn_job['now_ms']-1_000,
                'nodes':[{'price':2000.0,'activity':16.0,'share':0.2}],
            }
            complete=api('/v1/gateway/hvn/result',{'id':hvn_job['id'],'report':hvn_report})
            assert complete['ok'] and not complete['trade_authorized']
            hvn_result=api('/v1/hvn')
            assert not hvn_result['pending'] and hvn_result['report']==hvn_report
            assert not hvn_result['report_stale'] and hvn_result['report_age_ms']>=1000
            assert api('/v1/status')['requested_run'] is False
            snapshot.update(bid=target+.1,ask=target+.2);api('/v1/gateway/snapshot',snapshot)
            paper=api('/v1/status')['paper'];assert paper['trades']==1 and paper['realized']>0
            assert isinstance(api('/v1/opportunities/history')['setups'], list)
            # Research JSONL is now exported via a read-only worker. Status
            # polling must keep responding while the export owns its archive
            # slot; no account or broker effects are permitted here.
            status_latencies_ms=[]
            busy_observations=0
            with ThreadPoolExecutor(max_workers=2) as pool:
                research_export_future=pool.submit(api,'/v1/export',{},timeout=60)
                # Synthetic local HTTP envelope only; this is NOT MT5 or
                # broker order_send latency. Never assert universal P99.
                for _ in range(48):
                    started=time.perf_counter()
                    status_sample=api('/v1/status',timeout=6)
                    status_latencies_ms.append((time.perf_counter()-started)*1000)
                    assert status_sample['state']=='stopped'
                    busy_observations+=int(status_sample.get('backup_operation',{}).get('busy') is True)
                export=Path(research_export_future.result(timeout=65)['path'])
            samples=sorted(status_latencies_ms)
            nearest_rank=lambda pct:round(samples[max(0,(len(samples)*pct+99)//100-1)],3)
            print('CORE_LOCAL_HTTP_STATUS_PROFILE '+json.dumps({
                'samples':len(samples),
                'p50_ms':nearest_rank(50),'p95_ms':nearest_rank(95),
                'p99_ms':nearest_rank(99),'max_ms':round(samples[-1],3),
                'backup_busy_observations':busy_observations,
                'scope':'synthetic localhost Core status, not MT5 broker execution'
            }),flush=True)
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
            with ThreadPoolExecutor(max_workers=2) as pool:
                learning_export_future=pool.submit(api,'/v1/learning/export',{},timeout=60)
                status_during_learning_export=api('/v1/status')
                dataset=learning_export_future.result(timeout=65)
            assert status_during_learning_export['state']=='stopped'
            dataset_path=Path(dataset['path'])
            assert (dataset_path/'dataset.jsonl').is_file()
            manifest=json.loads((dataset_path/'manifest.json').read_text())
            assert manifest['feature_schema']=='gold-m1-feature-v1'
            assert manifest['rows']==dataset['rows']
            assert dataset['excluded_legacy_or_unverified_rows']>=0

            # P1-8G: the backup writer uses its own SQLite connection. The
            # Core must still serve status while the online backup runs.
            with ThreadPoolExecutor(max_workers=2) as pool:
                writing=pool.submit(api,'/v1/backup',{},timeout=60)
                status_during_backup=api('/v1/status')
                backup=writing.result(timeout=65)
            assert status_during_backup['state']=='stopped'
            folder=Path(backup['path'])
            assert (folder/'config.json').exists()
            # A validated backup is sealed. sqlite3.connect(path) opens a
            # read-write WAL database and CREATES -wal/-shm sidecars, which
            # correctly makes the sealed archive invalid on later inspection.
            # Use immutable read-only mode and explicitly close the handle.
            sealed_db_uri=(folder/'events.sqlite').resolve().as_uri()+'?mode=ro&immutable=1'
            with closing(sqlite3.connect(sealed_db_uri,uri=True)) as db:
                assert db.execute('PRAGMA integrity_check').fetchone()[0]=='ok'
                assert db.execute('SELECT COUNT(*) FROM events').fetchone()[0]>3
                assert db.execute('SELECT COUNT(*) FROM chronological_evaluations').fetchone()[0]==1
                assert db.execute('SELECT COUNT(*) FROM learning_review_records').fetchone()[0]==1
                assert db.execute('SELECT COUNT(*) FROM learning_predictions').fetchone()[0]==1
                assert db.execute("SELECT COUNT(*) FROM learning_slots WHERE slot='champion' AND model_id IS NOT NULL").fetchone()[0]==0
            assert not list(folder.glob('*.sealed'))
            assert not list(folder.glob('events.sqlite-*')), "Immutable check created SQLite sidecars"
            # Backup integrity checks are read-only and served by their
            # bounded worker, while Core status remains responsive.
            with ThreadPoolExecutor(max_workers=2) as pool:
                listing_future=pool.submit(api,'/v1/backups',timeout=60)
                core_status=api('/v1/status')
                listing=listing_future.result(timeout=65)
            assert core_status['state']=='stopped'
            assert any(item.get('name')==backup['name'] and item.get('valid') is True
                       for item in listing['backups'])
            checked=api('/v1/backup/restore/validate',{'name':backup['name']},timeout=60)
            assert checked['valid'] is True and checked['name']==backup['name']
            try:api('/v1/backup/restore/validate',{'name':'missing-backup-fixture'})
            except urllib.error.HTTPError as e:assert e.code==400
            else:raise AssertionError('Invalid backup was not rejected')
            # A self-contained snapshot export/import is a separate writer
            # operation, and must never mutate the current Engine database.
            brain=api('/v1/brain-snapshot/export',{},timeout=60)
            assert brain['secrets_included'] is False
            archive=Path(brain['path'])
            assert archive.is_file() and archive.stat().st_size==brain['bytes']
            inbox=Path(directory)/'brain_imports'
            inbox.mkdir(exist_ok=True)
            uploaded=inbox/'smoke-transfer.btbrain'
            shutil.copyfile(archive,uploaded)
            imported=api('/v1/brain-snapshot/import',{'name':uploaded.name},timeout=60)
            assert imported['imported'] is True and imported['restore_required'] is True
            assert not uploaded.exists()
            imported_folder=Path(directory)/'backups'/imported['backup_name']
            assert (imported_folder/'events.sqlite').is_file()
            checked_import=api('/v1/backup/restore/validate',
                               {'name':imported['backup_name']},timeout=60)
            assert checked_import['valid'] is True
            frozen=api('/v1/learning/control',{'frozen':True})
            assert frozen['frozen'] and frozen['trading_unchanged'] and frozen['cutoff_ms']>0
            assert api('/v1/status')['learning_control']['frozen']
            assert api('/v1/learning/control',{'frozen':True})['cutoff_ms']==frozen['cutoff_ms']
            for route,body in (('/v1/studio/run',{}),('/v1/studio/auto',{'enabled':True}),('/v1/learning/register',{})):
                try:api(route,body)
                except urllib.error.HTTPError as e:assert e.code==400
                else:raise AssertionError('Learning write was accepted while globally frozen: '+route)
            events_before=api('/v1/events')['events']
            api('/v1/shutdown',{});process.wait(timeout=5)
            assert process.returncode==0
            process=subprocess.Popen([str(BINARY)],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            for _ in range(50):
                try:status=api('/v1/status');break
                except OSError:time.sleep(.1)
            assert status['state']=='stopped' and not status['execution_enabled']
            assert status['learning_control']['frozen'] and status['learning_control']['cutoff_ms']==frozen['cutoff_ms']
            assert api('/v1/learning/control',{'frozen':False})['frozen'] is False
            api('/v1/gateway/snapshot',learning_ids['inference_snapshot'])
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
            assert inference['predictions']==1 and inference['shadow_effect'] is False, inference
            assert inference['broker_effect'] is False
            assert len(api('/v1/events')['events'])>len(events_before)
            api('/v1/shutdown',{});process.wait(timeout=5)
            assert process.returncode==0
            print('PASS: lifecycle/auth/staleness/shadow/config atomicity/risk mutation guard; backtest UTC chunking, cancel, account isolation, asynchronous completion, full export, deterministic offline replay, CES/Meta-v2 experiments, chronological evidence, challenger review eligibility, cross-language feature parity, read-only inference persistence/isolation, news API/settings, backup and restart persistence')
        finally:
            if process.poll() is None:process.terminate();process.wait(timeout=5)

if __name__=='__main__':main()
