"""Bounded read-only history; preserve actual MT5 calendar boundaries."""
from datetime import datetime, timezone
import math
TIMEFRAMES=('M1','M2','M3','M4','M5','M6','M10','M12','M15','M20','M30','H1','H2','H3','H4','H6','H8','H12','D1','W1','MN1')
def collect_frame(mt5,job,now=None):
    now=now or datetime.now(timezone.utc)
    out={k:job[k] for k in ('login','server','symbol','currency','timeframe')}
    out.update(captured_ms=int(now.timestamp()*1000),current_open=0,bars=[],error='')
    info=mt5.symbol_info(job['symbol'])
    if info is None:raise ValueError('Broker symbol unavailable')
    out['contract']=dict(point=info.point,tick_size=info.trade_tick_size,tick_loss=info.trade_tick_value_loss,tick_profit=info.trade_tick_value_profit,volume_min=info.volume_min,volume_step=info.volume_step,volume_max=info.volume_max)
    if any(not math.isfinite(v) or v<=0 for v in out['contract'].values()):raise ValueError('Broker contract unavailable')
    def identity():
        a=mt5.account_info()
        return (a.login,a.server,a.currency) if a else None
    expected=(job['login'],job['server'],job['currency'])
    try:
        if job['timeframe'] not in TIMEFRAMES or identity()!=expected:raise ValueError('Account changed or unsupported timeframe')
        rates=mt5.copy_rates_from_pos(job['symbol'],getattr(mt5,'TIMEFRAME_'+job['timeframe']),0,601)
        if rates is None or len(rates)<2:raise ValueError('History unavailable; open chart or increase MT5 Max bars')
        boundary=int(rates[-1]['time'])
        if boundary<=0 or boundary>now.timestamp()+2:raise ValueError('Invalid current-bar boundary')
        out['current_open']=boundary;rows=[]
        for r in rates[:-1]:
            names=getattr(getattr(r,'dtype',None),'names',()) or r.keys()
            row=dict(time=int(r['time']),open=float(r['open']),high=float(r['high']),low=float(r['low']),close=float(r['close']),tick_volume=int(r['tick_volume']),spread_points=int(r['spread']),real_volume=int(r['real_volume']) if 'real_volume' in names else 0)
            if row['time']>=boundary or any(not math.isfinite(row[k]) or row[k]<=0 for k in ('open','high','low','close')):raise ValueError('Invalid or unfinished bar')
            rows.append(row)
        if identity()!=expected:raise ValueError('Account changed during history read')
        out['bars']=rows
    except (AttributeError,TypeError,ValueError,RuntimeError) as e:
        out['bars']=[];out['error']=str(e)[:1000]
    return out
