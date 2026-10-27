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
        def prices(values):
            for value in values:
                time.sleep(.01)
                snapshot.update(bid=value,ask=value+.01)
                broker.tick.bid=value;broker.tick.ask=value+.01
                publish()
        def pulse(value):
            time.sleep(.01)
            broker.tick.bid=value;broker.tick.ask=value+.01
            body=dict(login=17,server='Demo',currency='USD',symbol='XAUUSD',bid=value,ask=value+.01,
                quote_time_ms=clock_ms(),tick_age_ms=0)
            request=urllib.request.Request('http://127.0.0.1:18741/v1/execution/pulse',method='PUT',
                data=json.dumps(body).encode(),headers={'Authorization':'Bearer '+key,'Content-Type':'application/json'})
            with opener.open(request,timeout=8) as response:return json.load(response)
        try:
            boot()
            config=api('/v1/config')['config'];config['trading'].update(mode='sniper',sniper_brain='learner')
            api('/v1/trading/settings',dict(trading=config['trading'],commission_per_lot=7.))
            snapshot.update(demo=True,balance=100.,equity=100.)
            publish();api('/v1/execution/settings',dict(target='demo',deviation_points=20))
            assert brain('sniper_learner')['magic']==410617
            prices([2053.80,2053.84,2053.88])
            assert brain('system')['latest']['signal']['ready'],brain('system')
            assert api('/v1/execution/gateway')['command'] is None
            # Control scope and actual effect: disable learner participation, then
            # prove it cannot open through the system before re-enabling it.
            control=dict(context=api('/v1/execution/brains')['context'],brain='sniper_learner',mode='sniper',control=dict(advisor=False,paper=False))
            api('/v1/execution/brain-control',control)
            api('/v1/start',{});time.sleep(1.05);prices([2053.80,2053.84,2053.88])
            assert not brain('system')['latest']['signal']['ready']
            assert api('/v1/execution/gateway')['command'] is None
            observed=api('/v1/execution/brains')['brains'][0]['sniper_opportunities']
            assert observed['sampled_observations']>=1 and observed['demo_only']
            assert observed['no_execution_effect'] and not observed['learning_effect']
            try: api('/v1/execution/brain-control',control)
            except urllib.error.HTTPError: pass
            else: raise AssertionError('Running control mutation must fail')
            api('/v1/stop',{})
            control['control']=dict(advisor=True,paper=True)
            api('/v1/execution/brain-control',control)
            view=api('/v1/execution/brains')
            assert len(view['brains'])==17 and view['trading_mode']=='sniper'
            assert all('activity' in b and 'paper_stats' in b for b in view['brains'])
            prices([2053.80,2053.84,2053.88])
            api('/v1/start',{})
            audit_before=api('/v1/execution/brains')['brains'][0]['sniper_opportunities']['sampled_observations']
            work=pulse(2053.92);assert work['pulse_fast_path']
            assert api('/v1/execution/brains')['brains'][0]['sniper_opportunities']['sampled_observations']==audit_before
            cmd=work['command'];assert cmd and cmd['brain']=='system',brain('system')
            selection=cmd['signal']['coordination']['selection']
            assert selection['brain']=='sniper_learner' and selection['sniper_policy']=='impulse'
            assert selection['sniper_target_mode']=='fixed'
            assert abs(selection['sniper_target_pips'] - 5.)<1e-8
            assert 0<selection['selection_probability']<=1
            result=submit(cmd);assert result['state']=='filled' and result['fast_path'],result
            submit(cmd);assert len(broker.sends)==1
            api('/v1/stop',{})
            now=clock_ms();volume=result['volume']
            entry=dict(ticket=77,position_id=55,symbol='XAUUSD',magic=410600,
                type=broker.DEAL_TYPE_BUY,entry=broker.DEAL_ENTRY_IN,
                volume=volume,price=2053.92,
                time_msc=now-100,profit=0.,commission=-.035,swap=0.,fee=0.,reason=3)
            partial=dict(entry,ticket=78,magic=0,type=broker.DEAL_TYPE_SELL,
                entry=broker.DEAL_ENTRY_OUT,volume=volume/2,price=2053.86,
                time_msc=now,profit=-.1,commission=-.0175)
            sync=dict(login=17,server='Demo',currency='USD',trade_mode=0,until_ms=now,deals=[entry,partial])
            api('/v1/execution/sync',sync);publish()
            assert not api('/v1/execution')['learning_review']
            final=dict(partial,ticket=79,time_msc=now+1)
            sync.update(until_ms=now+1,deals=[entry,partial,final])
            api('/v1/execution/sync',sync);prices([2053.96])
            samples=api('/v1/execution')['learning_review']
            assert {v['brain'] for v in samples}=={'sniper_learner','system'},samples
            assert abs(next(v for v in samples if v['brain']=='sniper_learner')['net']+.27)<1e-8
            def learned_count():
                return brain('system')['latest']['signal']['coordination']['selection']['broker_samples']
            assert learned_count()==1,brain('system')
            api('/v1/execution/sync',sync);prices([2054.00]);assert learned_count()==1
            api('/v1/shutdown',{});process.wait(timeout=10);boot();prices([2054.04,2054.08,2054.12])
            assert not api('/v1/status')['requested_run'];assert learned_count()==1
            # The Demo-only bounded optimizer is enabled explicitly and uses
            # four configured pip arms; it cannot invent out-of-range targets.
            config['trading'].update(sniper_target_mode='adaptive',
                                     sniper_min_target_pips=3.0,sniper_max_target_pips=9.0)
            saved=api('/v1/trading/settings',dict(trading=config['trading'],commission_per_lot=7.))
            assert saved['config']['trading']['sniper_target_mode']=='adaptive'
            prices([2054.13,2054.17,2054.21])
            latest=brain('system')['latest']['signal']
            assert latest['ready'],latest
            selection=latest['coordination']['selection']
            assert selection['sniper_target_mode']=='adaptive',selection
            assert 3.<=selection['sniper_target_pips']<=9.
            assert len(selection['sniper_target_candidates_pips'])==4
            assert selection['sniper_target_samples']==0,selection
            # A previously separate Sniper Learner may now only advise the
            # selected-mode system coordinator. No independent MT5 order can
            # bypass the selected mode, even with legacy SOLO preferences.
            try:
                configure('sniper_learner',enabled=True)
            except urllib.error.HTTPError:
                pass
            else:
                raise AssertionError('Non-system SOLO must be blocked')
            assert brain('sniper_learner')['role']=='advisor'
            api('/v1/start',{});prices([2054.16])
            work=api('/v1/execution/gateway')
            if work['command'] is not None:
                assert work['command']['brain']=='system',work
                # This is a simulated test without an actual broker send;
                # finalize the dispatched probe before changing mode.
                api('/v1/execution/result',dict(id=work['command']['id'],state='cancelled'))
            assert all(c['request']['brain']!='sniper_learner' or c['request']['kind']!='open'
                for c in api('/v1/execution')['commands'])
            api('/v1/stop',{})
            # A change of mode cannot execute the Sniper specialist.
            config['trading']['mode']='scalping'
            api('/v1/trading/settings',dict(trading=config['trading'],commission_per_lot=7.))
            prices([2054.20,2054.24,2054.28])
            assert not brain('sniper_learner')['latest']['signal']['ready']
            snapshot['login']=99;publish();assert api('/v1/execution/gateway')['command'] is None
            report=dict(result='PASS',version=VERSION,broker='simulated; no native MT5 acceptance',checks=[
                'explicit Sniper learner selector; STOP never submits','Fast Pulse tick engine reaches authorized order_send',
                'duplicate never resends; partial close never trains','net fees credited only to specialist and owner',
                'restart restores policy history and STOP','SOLO blocked: only selected mode coordinator executes',
                'inactive outside Sniper; account isolation',
                'fixed 5-pip target; user-bounded adaptive 3–9-pip Demo-only target selection'])
            out=ROOT/'test-output/sniper-learner-integration.json';out.parent.mkdir(exist_ok=True);out.write_text(json.dumps(report,indent=2))
            print(json.dumps(report))
        finally:
            executor.close()
            if process and process.poll() is None:process.terminate();process.wait(timeout=10)
            if sys.exc_info()[0]:log.seek(0);print(log.read().decode(errors='replace'),file=sys.stderr)
            log.close()
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--exe');check(parser.parse_args().exe)
