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
            current=clock_ms()
            snapshot.update(quote_time_ms=current,tick_age_ms=0)
            # This acceptance test can run while Windows compiles/packages under
            # heavy CI load. A synthetic snapshot must never pretend its last
            # completed candle is fresh merely by advancing captured_ms.
            # Rebuild only expired fixture frames with actual completed times;
            # production MT5 market data is not modified by this script.
            for name,seconds,side in [('m1',60,1),('m5',300,-1),('m15',900,-1)]:
                series=snapshot['market'][name]
                if not series or current-(series[-1]['time']+seconds)*1000>seconds*1000+15_000:
                    snapshot['market'][name]=bars(seconds,side)
            snapshot['market']['captured_ms']=current
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
            # The same commission contract must survive Core API, Rust cost
            # calculation and the broker Python path. A zero-rate account
            # is zero in modeled fees, never a waiver of actual broker fees.
            for rate in (7.,0.):
                for lots in (.01,.10,1.):
                    preview=api('/v1/trading/preview',dict(
                        commission_per_lot=rate,commission_basis='round_trip',
                        pip_points=10.,sniper_pip_price=.10,
                        lots=lots,slippage_ticks=0.))
                    assert abs(preview['commission']-rate*lots)<1e-9,preview
                    e=preview['commission_examples']
                    assert abs(e['one_hundredth_lot']-rate*.01)<1e-9,preview
                    assert abs(e['one_tenth_lot']-rate*.1)<1e-9,preview
                    assert abs(e['one_lot']-rate)<1e-9,preview
                    if preview.get('costs'):
                        assert abs(preview['costs']['commission']-rate*lots)<1e-9,preview
            assert len(api('/v1/execution')['brains'])==17
            assert brain('system')['role']=='coordinator' and brain('trend')['role']=='advisor'
            assert brain('liquidity')['role']=='advisor' and not brain('liquidity')['solo_enabled']
            assert brain('trend_pullback')['role']=='advisor' and not brain('trend_pullback')['solo_enabled']
            # All historical SOLO flags are now deliberately rejected. Advisors
            # continue collecting paper outcomes; only the selected-mode
            # coordinator may produce real MT5 commands.
            try:
                configure('trend',enabled=True,lots=.1)
            except urllib.error.HTTPError:
                pass
            else:
                raise AssertionError('SOLO must be rejected in selected-mode execution')
            assert brain('trend')['role']=='advisor'
            configure('trend',enabled=False,lots=.1)
            assert api('/v1/execution/gateway')['command'] is None
            assert not api('/v1/status')['requested_run']
            # Align the primary scalping mode's completed confirmation frames.
            snapshot['market']['m5']=bars(300,1)
            snapshot['market']['m15']=bars(900,1)
            publish()
            api('/v1/start',{})
            publish()
            cmd=api('/v1/execution/gateway')['command']
            assert cmd and cmd['brain']=='system' and cmd['trading_mode']=='scalping', brain('system')
            assert cmd['signal']['reason'],cmd
            assert api('/v1/execution/authorize',{'id':cmd['id']})['authorized']
            broker.raise_send=True
            result=submit(cmd)
            assert result['state']=='unknown',result
            submit(cmd)
            assert len(broker.sends)==1
            broker.raise_send=False
            broker.deals=[N(ticket=77,magic=410600,symbol='XAUUSD',volume=.01,comment='BT:'+cmd['id'][:20])]
            result=submit(cmd)
            assert result['state']=='filled',result
            assert len(broker.sends)==1
            api('/v1/stop',{})
            # Broker's real entry and exit history, including both fee legs.
            now=clock_ms()
            entry=dict(ticket=77,position_id=55,symbol='XAUUSD',magic=410600,
                       entry=0,type=0,time_msc=now-100,volume=result['volume'],price=2053.9,
                       profit=0.,commission=-.35,swap=0.,fee=0.,reason=3)
            exit_deal=dict(entry,ticket=78,magic=0,entry=1,time_msc=now,
                           price=2055.,profit=10.,commission=-.35,swap=-.2)
            sync=dict(login=17,server='Demo',currency='USD',until_ms=now,deals=[entry,exit_deal])
            api('/v1/execution/sync',sync)
            api('/v1/execution/sync',sync)
            journal=api('/v1/execution/trades',dict(offset=0,limit=50))
            assert journal['total']==1,journal
            trade=journal['rows'][0]
            assert trade['status']=='CLOSED',trade
            assert trade['origin']['owner']=='system',trade
            assert trade['origin']['trading_mode']=='scalping',trade
            assert abs(trade['broker_net_booked']-9.1)<1e-7,trade
            assert 'shadow_paper' in api('/v1/execution')
            # Restart does not erase the real account journal or activate MT5.
            api('/v1/shutdown',{})
            process.wait(timeout=10)
            boot()
            publish()
            assert not api('/v1/status')['requested_run']
            restored=api('/v1/execution/trades',dict(offset=0,limit=50))
            assert restored['rows'][0]['origin']['trading_mode']=='scalping',restored
            assert abs(restored['rows'][0]['broker_net_booked']-9.1)<1e-7
            # Switching MT5 accounts never enables broker execution.
            snapshot['login']=99;publish()
            assert not api('/v1/status')['execution_enabled']
            assert api('/v1/execution/gateway')['command'] is None
            # Simulated Demo -> REAL operator promotion must require two
            # independent typed confirmations. There is NO real MT5 connection.
            snapshot.update(login=990024,server='Real-Test',account_trade_mode=2,demo=False)
            publish()
            for v in [
                dict(target='live',deviation_points=20),
                dict(target='live',deviation_points=20,confirm_live_login='99002'),
            ]:
                try: api('/v1/execution/settings',v)
                except urllib.error.HTTPError: pass
                else: raise AssertionError('REAL bind accepted missing/wrong login confirmation')
            assert api('/v1/execution')['settings']['target']=='demo'
            api('/v1/execution/settings',
                dict(target='live',deviation_points=20,confirm_live_login='990024'))
            assert api('/v1/execution')['settings']['target']=='live'
            for body in ({},{'confirm_live_login':'17'},{'confirm_live_login':'990024'}):
                try: api('/v1/start',body)
                except urllib.error.HTTPError: pass
                else: raise AssertionError('REAL START accepted missing login or disabled portfolio')
            api('/v1/execution/portfolio',dict(enabled=True,max_positions=1,max_lots=.02,max_risk_percent=1.))
            assert api('/v1/start',dict(confirm_live_login='990024'))['requested_run']
            api('/v1/stop',{})
            snapshot.update(login=990025);publish()
            assert not api('/v1/status')['execution_enabled']
            api('/v1/shutdown',{})
            process.wait(timeout=10)
            report=dict(result='PASS',version=VERSION,broker='simulated; no native MT5 acceptance',checks=[
                'only selected-mode system coordinator may dispatch MT5',
                'former SOLO brains fail closed; their evaluation remains research-only',
                'Core+gateway unknown send never blindly retries',
                'durable real trade journal attributes mode and owner, booked profit and all deal fees',
                'closed trades persist after restart and can be read with pagination',
                'account switch fails closed; STOP at restart',
                'real-money account bind rejects missing or wrong login, REAL START independently requires exact login and enabled portfolio',
                'account switch disables execution even after explicit REAL START',
                'Core real-quote preview matches commission 7 and 0 across 0.01, 0.10 and 1.00 lots'])
            out=ROOT/'test-output/execution-integration.json';out.parent.mkdir(exist_ok=True);out.write_text(json.dumps(report,indent=2))
            print(json.dumps(report))
        finally:
            executor.close()
            if process and process.poll() is None:process.terminate();process.wait(timeout=10)
            if sys.exc_info()[0]:log.seek(0);print(log.read().decode(errors='replace'),file=sys.stderr)
            log.close()
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--exe');check(parser.parse_args().exe)