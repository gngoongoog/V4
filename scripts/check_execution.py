"""Real Core + simulated MT5 acceptance. Never connects to or submits to a broker."""
from __future__ import annotations
import argparse, json, os, subprocess, sys, tempfile, time, urllib.request, urllib.error
from pathlib import Path
from types import SimpleNamespace as N
ROOT = Path(__file__).resolve().parents[1]
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
            config=api('/v1/config')['config']; config['trading']['mode']='scalping'
            api('/v1/trading/settings',dict(trading=config['trading'],commission_per_lot=7.))
            config['risk']['max_lots']=.01;api('/v1/risk',config['risk'])
            publish();api('/v1/execution/settings',dict(target='demo',deviation_points=20))
            assert len(api('/v1/execution')['brains'])==14
            assert brain('system')['role']=='coordinator' and brain('trend')['role']=='advisor'
            configure('trend',enabled=True,lots=.1,max_hold_seconds=2)
            publish();assert not api('/v1/status')['requested_run'];assert api('/v1/execution/gateway')['command'] is None
            assert brain('trend')['role']=='solo'
            members=brain('system')['latest']['signal']['coordination']['members']
            assert all(m['id']!='trend' for m in members), members
            api('/v1/start',{});publish()
            cmd=api('/v1/execution/gateway')['command'];assert cmd['brain']=='trend' and cmd['lots']==.1,cmd
            broker.raise_send=True
            result=submit(cmd);assert result['state']=='unknown',result
            submit(cmd);assert len(broker.sends)==1
            broker.raise_send=False
            broker.deals=[N(ticket=77,magic=410603,symbol='XAUUSD',volume=.1,comment='BT:'+cmd['id'][:20])]
            result=submit(cmd);assert result['state']=='filled',result
            assert len(broker.sends)==1
            entry=dict(ticket=77,position_id=55,symbol='XAUUSD',magic=410603,entry=0,profit=0.,commission=-.35,swap=0.,fee=0.)
            sync=dict(login=17,server='Demo',currency='USD',until_ms=clock_ms(),deals=[entry])
            api('/v1/execution/sync',sync);publish()
            assert next(c for c in api('/v1/execution')['commands'] if c['request']['id']==cmd['id'])['state']=='filled'
            configure('trend',enabled=False,max_hold_seconds=1800)
            assert brain('trend')['role']=='returning'
            opened=clock_ms()
            position=dict(ticket=55,identifier=55,magic=410603,opened_ms=opened,symbol='XAUUSD',side='BUY',volume=.1,price_open=2053.9,price_current=2053.8,profit=-1.,sl=2052.,tp=2056.)
            snapshot['positions']=[position];broker.positions=[N(**dict(position,type=0))];publish()
            assert brain('trend')['role']=='returning'
            api('/v1/stop',{});time.sleep(2.15);publish()
            close=api('/v1/execution/gateway')['command'];assert close['kind']=='close' and close['position']==55,close
            assert submit(close)['state']=='filled'
            snapshot['positions']=[];broker.positions=[];publish()
            assert brain('trend')['role']=='advisor'
            assert any(m['id']=='trend' for m in brain('system')['latest']['signal']['coordination']['members'])
            exit_deal=dict(entry,ticket=78,magic=0,entry=1,profit=10.,commission=-.35,swap=-.2)
            sync.update(until_ms=clock_ms(),deals=[entry,exit_deal]);api('/v1/execution/sync',sync);api('/v1/execution/sync',sync)
            stats=brain('trend')['stats'];assert stats['deals']==2 and stats['closed_positions']==1 and abs(stats['net_booked']-9.1)<1e-8,stats
            # A queued primary decision must be cancelled by changing its membership.
            snapshot['market']['m5']=bars(300,1);snapshot['market']['m15']=bars(900,1)
            api('/v1/start',{});publish()
            primary=next(c for c in api('/v1/execution')['commands'] if c['state']=='queued' and c['request']['brain']=='system')
            configure('momentum',enabled=True)
            assert next(c for c in api('/v1/execution')['commands'] if c['request']['id']==primary['request']['id'])['state']=='cancelled'
            publish();stale=api('/v1/execution/gateway')['command'];assert stale['brain']=='momentum',stale
            configure('gann',enabled=True)
            assert not api('/v1/execution/authorize',{'id':stale['id']})['authorized']
            assert submit(stale)['state']=='cancelled';assert len(broker.sends)==2
            # Unknown intent remains a reconciliation task across Core restart, never a resend.
            configure('gann',enabled=False);configure('momentum',timeframe='M5')
            snapshot['market']['m5']=bars(300,-1);snapshot['market']['m15']=bars(900,-1);publish()
            unknown=api('/v1/execution/gateway')['command'];assert unknown['brain']=='momentum',unknown
            api('/v1/execution/result',dict(id=unknown['id'],state='unknown'))
            api('/v1/shutdown',{});process.wait(timeout=10);boot();publish()
            assert not api('/v1/status')['requested_run']
            persisted=api('/v1/execution/gateway')['command'];assert persisted['id']==unknown['id'] and persisted['state']=='unknown'
            assert submit(persisted)['state']=='unknown' and len(broker.sends)==2
            assert abs(brain('trend')['stats']['net_booked']-9.1)<1e-8
            snapshot['login']=99;publish()
            assert not api('/v1/status')['execution_enabled'];assert api('/v1/execution/gateway')['command'] is None
            assert brain('trend')['stats']['deals']==0
            api('/v1/shutdown',{});process.wait(timeout=10)
            report=dict(result='PASS',version='0.6.0',broker='simulated; no native MT5 acceptance',checks=[
                '14 identities; default coordinator and advisors','SOLO exclusion; no implicit global START',
                'standalone M1 trend against opposing M5','optional risk caps off',
                'Core+gateway single submission','unknown-to-fill history retains position barrier','entry history does not release fill barrier',
                'pending return and frozen timeout exit while STOPPED','advisor rejoins actual votes after close',
                'manual-exit ownership; actual fees; history deduplication','queued decision invalidation',
                'dispatched revision checked before send','unknown intent persists without resend',
                'restart stopped; config/PnL persistence','account isolation'])
            out=ROOT/'test-output/execution-integration.json';out.parent.mkdir(exist_ok=True);out.write_text(json.dumps(report,indent=2))
            print(json.dumps(report))
        finally:
            executor.close()
            if process and process.poll() is None:process.terminate();process.wait(timeout=10)
            if sys.exc_info()[0]:log.seek(0);print(log.read().decode(errors='replace'),file=sys.stderr)
            log.close()
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--exe');check(parser.parse_args().exe)
