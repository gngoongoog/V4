"""Real Core + simulated MT5 acceptance. Never connects to or submits to a broker."""
from __future__ import annotations
import argparse, json, os, subprocess, sys, tempfile, time, tomllib, urllib.request, urllib.error
from pathlib import Path
from types import SimpleNamespace as N
ROOT = Path(__file__).resolve().parents[1]
VERSION = tomllib.loads((ROOT/'Cargo.toml').read_text(encoding='utf-8'))['workspace']['package']['version']
sys.path[:0] = [str(ROOT/'gateway'), str(ROOT/'tests'), str(ROOT/'scripts')]
from mt5_gateway import empty_snapshot
from execution import BrokerExecutor, clock_ms
from test_broker_execution import Broker

def check(exe=None):
    binary = Path(exe or ROOT/'target/debug/braintrade-core').resolve()
    args = [str(binary)] + (['--core'] if exe else [])
    with tempfile.TemporaryDirectory(prefix='braintrade-execution-') as directory:
        env = dict(os.environ, BRAINTRADE_DATA_DIR=directory, BRAINTRADE_GATEWAY_DISABLED='1', BRAINTRADE_NEWS_OFFLINE='1')
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        key = ''; process = None; log = open(Path(directory)/'core.log', 'w+b')
        def api(path, body=None):
            request = urllib.request.Request('http://127.0.0.1:18741'+path,
                data=None if body is None else json.dumps(body).encode(),
                headers={'Authorization':'Bearer '+key, 'Content-Type':'application/json'})
            with opener.open(request, timeout=8) as response: return json.load(response)
        def boot():
            nonlocal key, process
            process = subprocess.Popen(args, env=env, stdout=log, stderr=log)
            for _ in range(150):
                try:
                    if os.name == 'nt':
                        from check_windows_portable import unseal
                        key = unseal(Path(directory)/'ipc.key')
                    else: key = Path(directory,'ipc.key').read_text()
                    api('/v1/status'); return
                except OSError:
                    if process.poll() is not None: break
                    time.sleep(.1)
            raise AssertionError('Core failed to start')
        def brain(name): return next(b for b in api('/v1/execution')['brains'] if b['config']['id']==name)
        def configure(name, **values):
            cfg = dict(brain(name)['config'], **values)
            response = api('/v1/execution/brain', cfg)
            assert not response['start']
            return cfg
        def bars(seconds, side):
            end = clock_ms()//1000//seconds*seconds
            return [dict(time=end-(180-i)*seconds,open=2000+side*i*.3,high=2000+side*i*.3+.4,
                low=2000+side*i*.3-.4,close=2000+side*i*.3+side*.1,tick_volume=100) for i in range(180)]
        snapshot = empty_snapshot('')
        snapshot.update(connected=True, login=17, server='Demo', currency='USD', symbol='XAUUSD',
            account_trade_mode=0,margin_mode=2,trade_allowed=True,trade_expert=True,balance=10000.,equity=10000.,
            today_net=0.,point=.01,tick_size=.01,tick_value_loss=1.,tick_value_profit=1.,
            volume_min=.01,volume_step=.01,volume_max=100.,bid=2053.8,ask=2053.9,
            market=dict(captured_ms=clock_ms(),m1=bars(60,1),m5=bars(300,-1),m15=bars(900,-1),error=''))
        def publish():
            snapshot.update(quote_time_ms=clock_ms(),tick_age_ms=0)
            snapshot['market']['captured_ms']=clock_ms()
            api('/v1/gateway/snapshot',snapshot)
        broker=Broker();broker.tick.bid=2053.8;broker.tick.ask=2053.9
        executor=BrokerExecutor(Path(directory)/'send-journal.sqlite')
        def submit(cmd):
            broker.tick.time_msc=clock_ms()
            result=executor.execute(broker,cmd,lambda q:api('/v1/execution/authorize',{'id':q})['authorized'])
            api('/v1/execution/result',result)
            return result
        try:
            boot()
            config=api('/v1/config')['config'];config['trading'].update(mode='expert',demo_flexible=True)
            api('/v1/trading/settings',dict(trading=config['trading'],commission_per_lot=7.))
            snapshot.update(demo=True,balance=100.,equity=100.)
            broker.account.margin_free=100.
            publish();api('/v1/execution/settings',dict(target='demo',deviation_points=20))
            configure('system',lots=.5)
            api('/v1/start',{});publish()
            command1=api('/v1/execution/gateway')['command'];assert command1 and command1['brain']=='system',brain('system')
            selected=command1['signal']['coordination']['selection']['brain']
            assert command1['demo_flexible'] and command1['learning']['trading_mode']=='expert'
            choice=command1['signal']['coordination']['selection']
            assert choice['policy']=='contextual_exploration_v3' and 0<choice['selection_probability']<=1
            assert command1['signal']['coordination']['student']['profile']=='baseline'
            assert command1['signal']['coordination']['student']['exit_candidates']==2
            result=submit(command1);assert result['state']=='filled',result
            assert api('/v1/execution/student')['total_experiences']==1, 'acknowledged fill must register before the next snapshot'
            assert abs(result['request']['volume']-.09)<1e-8,result
            assert len(broker.sends)==1
            # No blind duplicate before the first position is reconciled.
            submit(command1);assert len(broker.sends)==1
            api('/v1/stop',{})
            now=clock_ms()
            entry=dict(ticket=77,position_id=55,symbol='XAUUSD',magic=410600,entry=0,volume=.09,
                time_msc=now-2000,profit=0.,commission=-.315,swap=0.,fee=0.,reason=3)
            partial=dict(entry,ticket=78,magic=0,entry=1,volume=.04,time_msc=now-1000,profit=.1,commission=-.14,reason=5)
            sync=dict(login=17,server='Demo',currency='USD',until_ms=now,deals=[entry,partial])
            api('/v1/execution/sync',sync);publish()
            assert not api('/v1/execution')['learning_review']
            final=dict(partial,ticket=79,volume=.05,time_msc=now,profit=.1,commission=-.175)
            sync['deals']=[entry,partial,final]
            api('/v1/execution/sync',sync);publish()
            view=api('/v1/execution')
            samples=view['learning_review'];assert {v['brain'] for v in samples}=={selected,'system'},samples
            sample=next(v for v in samples if v['brain']==selected)
            assert sample['attribution']=='costs_erased_gross_profit',sample
            assert abs(sample['net']+.43)<1e-8,sample
            learned=brain(selected);assert learned['learning']['broker_samples']==1,learned
            assert learned['learned_weight']<1.,learned
            assert learned['learning']['context_samples']==1,learned
            student=api('/v1/execution/student')
            assert student['closed_broker_trades']==1 and student['total_experiences']==1,student
            assert student['experiences'][0]['brain']==selected
            assert abs(student['experiences'][0]['actual']['net']+.43)<1e-8
            assert student['recent_pattern_counts']['cost_drag']==1
            assert student['entry_timing']['research_only'] is True
            assert student['entry_timing']['total_opportunities']==1
            assert student['entry_timing']['delays_ms']==[0,250,1000]
            # Replaying history doesn't multiply samples or fake additional learning.
            api('/v1/execution/sync',sync);publish()
            assert len(api('/v1/execution')['learning_review'])==2
            api('/v1/shutdown',{});process.wait(timeout=10);boot();publish()
            assert not api('/v1/status')['requested_run']
            assert brain(selected)['learning']['broker_samples']==1
            assert brain(selected)['learned_weight']<1.
            assert api('/v1/execution/student')['closed_broker_trades']==1
            assert api('/v1/execution/student')['entry_timing']['total_opportunities']==1
            snapshot['login']=99;publish()
            assert not api('/v1/execution')['learning_review']
            assert api('/v1/execution/student')['total_experiences']==0
            assert api('/v1/execution/student')['entry_timing']['total_opportunities']==0
            assert api('/v1/execution/gateway')['command'] is None
            report=dict(result='PASS',version=VERSION,broker='simulated; no native MT5 acceptance',balance=100.,
                checks=['ready expert selects without consensus','Core authorization reaches adapter order_send',
                '100 USD free margin downsizes .50 to .09 under fixture contract',
                'duplicate dispatch does not resend','partial history does not train',
                'selected advisor only plus primary owner receive actual closed net outcome',
                'positive gross turned into loss by booked commission is explained',
                'negative outcome lowers next learned influence','restart preserves learning and STOP',
                'contextual exploration probability frozen in order',
                'student journal uses exact closed net and persists through restart',
                'account isolation'])
            out=ROOT/'test-output/flexible-demo-integration.json';out.parent.mkdir(exist_ok=True);out.write_text(json.dumps(report,indent=2))
            print(json.dumps(report))
        finally:
            executor.close()
            if process and process.poll() is None:process.terminate();process.wait(timeout=10)
            if sys.exc_info()[0]:log.seek(0);print(log.read().decode(errors='replace'),file=sys.stderr)
            log.close()
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--exe');check(parser.parse_args().exe)
