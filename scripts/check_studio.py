"""Real Core API test: modes, fees, history, research splits and restart persistence."""
import json,os,subprocess,tempfile,time,urllib.request,urllib.error,sys,math
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'gateway'))
from mt5_gateway import empty_snapshot

def check():
    with tempfile.TemporaryDirectory(prefix='braintrade-studio-') as d:
        env=dict(os.environ,BRAINTRADE_DATA_DIR=d,BRAINTRADE_NEWS_OFFLINE='1');env.pop('BRAINTRADE_GATEWAY_SCRIPT',None)
        binary=ROOT/'target/debug/braintrade-core'
        process=subprocess.Popen([str(binary)],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
        opener=urllib.request.build_opener(urllib.request.ProxyHandler({}));key=''
        def api(path,body=None):
            req=urllib.request.Request('http://127.0.0.1:18741'+path,data=None if body is None else json.dumps(body).encode(),headers={'Authorization':'Bearer '+key,'Content-Type':'application/json'})
            with opener.open(req,timeout=4) as r:return json.load(r)
        def ready():
            nonlocal key
            for _ in range(100):
                try:key=Path(d,'ipc.key').read_text();api('/v1/status');return
                except OSError:time.sleep(.1)
            raise AssertionError('Core startup failed')
        def rejects(path,body):
            try:api(path,body)
            except urllib.error.HTTPError as e:assert e.code==400
            else:raise AssertionError('Accepted invalid request '+path)
        try:
            ready();cfg=api('/v1/config')['config'];assert cfg['trading']['mode']=='expert'
            for mode in ['scalping','swing','expert','sniper']:
                cfg['trading']['mode']=mode;api('/v1/trading/settings',dict(trading=cfg['trading'],commission_per_lot=7.));assert api('/v1/config')['config']['trading']['mode']==mode
            p=api('/v1/trading/preview',dict(lots=.1,commission_per_lot=7.,pip_points=10.));assert abs(p['commission']-.7)<1e-12 and p['error']
            rejects('/v1/trading/preview',dict(lots=-1,commission_per_lot=7.,pip_points=10.))
            now=int(time.time()*1000);boundary=now//60000*60
            def bars(n,seconds=60):
                end=now//1000//seconds*seconds
                return [dict(time=end-(n-i)*seconds,open=100+math.sin(i*.2)*2+i*.01,high=100.4+math.sin(i*.2)*2+i*.01,low=99.6+math.sin(i*.2)*2+i*.01,close=100.1+math.sin(i*.2)*2+i*.01,tick_volume=100) for i in range(n)]
            snap=empty_snapshot('');snap.update(connected=True,login=1,server='S',symbol='XAUUSD',currency='USD',balance=10000.,equity=10000.,point=.01,tick_size=.01,tick_value_loss=1.,tick_value_profit=1.,volume_min=.01,volume_step=.01,volume_max=100.,bid=106.,ask=106.02,tick_age_ms=0,quote_time_ms=now,market=dict(captured_ms=now,m1=bars(180),m5=bars(180,300),m15=bars(180,900),error=''))
            api('/v1/gateway/snapshot',snap);p=api('/v1/trading/preview',dict(lots=.1,commission_per_lot=7.,pip_points=10.));assert abs(p['costs']['total']-1.3)<1e-8
            api('/v1/start',{});rejects('/v1/trading/settings',dict(trading=cfg['trading'],commission_per_lot=7.));api('/v1/stop',{})
            contract=dict(point=.01,tick_size=.01,tick_loss=1.,tick_profit=1.,volume_min=.01,volume_step=.01,volume_max=100.)
            f=dict(login=1,server='S',symbol='XAUUSD',currency='USD',timeframe='M1',captured_ms=now,current_open=boundary,contract=contract,bars=[dict(b,spread_points=2,real_volume=0) for b in bars(600)],error='')
            rejects('/v1/gateway/research/frame',dict(f,login=2));api('/v1/gateway/research/frame',f)
            bad=json.loads(json.dumps(f));bad['bars'][-1]['time']=boundary;rejects('/v1/gateway/research/frame',bad)
            assert api('/v1/studio/run',{'timeframe':'M1'})['queued']==1
            for _ in range(200):
                v=api('/v1/studio')
                if v['reports']:break
                time.sleep(.1)
            assert v['reports'],v
            report_id=v['reports'][0]['id'];r=api('/v1/studio/report',{'id':report_id});assert r['test_start']>r['validation_end']+24*60 and r['trials']==40
            snap['quote_time_ms']=int(time.time()*1000);api('/v1/gateway/snapshot',snap)
            assert api('/v1/studio/run',{'timeframe':'M1'})['queued']==0;assert len(api('/v1/studio')['frames'])==21
            api('/v1/studio/auto',{'enabled':True});assert api('/v1/studio')['auto'];api('/v1/studio/cancel',{})
            api('/v1/shutdown',{});process.wait(timeout=8)
            process=subprocess.Popen([str(binary)],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE);ready()
            assert api('/v1/config')['config']['trading']['mode']=='sniper';assert api('/v1/studio/report',{'id':report_id})==r
            snap['login']=2;api('/v1/gateway/snapshot',snap);assert api('/v1/studio')['reports']==[];assert not api('/v1/status')['execution_enabled']
            print('PASS: four modes, proportional/current costs, frame closure/account validation, 21 frames, async synthesis, purged test, no test reuse, restart and account isolation')
            api('/v1/shutdown',{});process.wait(timeout=8)
        finally:
            if process.poll() is None:process.terminate();process.wait(timeout=5)
            if process.returncode:print(process.stderr.read().decode(),file=sys.stderr)
if __name__=='__main__':check()
