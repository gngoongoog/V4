"""Aligned completed-bar correlation. Missing observations stay missing; no order APIs."""
from datetime import datetime, timezone
from itertools import combinations
import math

PERIODS = {'M1':60, 'M5':300, 'M15':900, 'H1':3600}
WINDOW = 240
MIN_PAIRS = 60


def aligned_returns(rows, times, seconds):
    prices = {}
    for row in rows:
        stamp, close = int(row['time']), float(row['close'])
        if stamp <= 0 or stamp % seconds or not math.isfinite(close) or close <= 0 or stamp in prices:
            raise ValueError('Invalid or duplicate completed candle')
        prices[stamp] = close
    return [math.log(prices[t])-math.log(prices[t-seconds]) if t in prices and t-seconds in prices else None for t in times]


def pair_statistics(a, b):
    if len(a)!=len(b) or any(v is not None and not math.isfinite(v) for values in (a,b) for v in values):
        raise ValueError('Returns must have matching finite windows')
    mask = [x is not None and y is not None for x,y in zip(a,b)]
    rows = [(x,y) for x,y in zip(a,b) if x is not None and y is not None]
    result = dict(samples=len(rows), coverage=len(rows)/len(a) if a else 0., mask=mask, correlation=None)
    if len(rows) < MIN_PAIRS:
        result['reason'] = 'Insufficient simultaneous completed returns'
        return result
    mx, my = (sum(row[i] for row in rows)/len(rows) for i in (0,1))
    xx = sum((x-mx)**2 for x,y in rows); yy = sum((y-my)**2 for x,y in rows)
    if xx <= 1e-24 or yy <= 1e-24:
        result['reason'] = 'Constant or near-constant returns'
    else:
        result['correlation'] = max(-1., min(1., sum((x-mx)*(y-my) for x,y in rows)/math.sqrt(xx*yy)))
        result['reason'] = 'Aligned log returns; descriptive, not a forecast'
    return result


def collect_correlation(mt5, job, now=None):
    now = now or datetime.now(timezone.utc)
    seconds = PERIODS[job['timeframe']]
    symbols = job['symbols']
    if not 1 <= len(symbols) <= 6 or len(set(symbols)) != len(symbols):
        raise ValueError('Expected one to six unique broker symbols')
    wanted = job['login'], job['server'], job['currency']
    def identity():
        account = mt5.account_info()
        return (account.login,account.server,account.currency) if account else None
    if identity() != wanted:
        raise ValueError('Account changed before correlation read')
    end = int(now.timestamp())//seconds*seconds-seconds
    times = [end-(WINDOW-1-i)*seconds for i in range(WINDOW)]
    series=[]
    for symbol in symbols:
        item=dict(symbol=symbol, returns=[None]*WINDOW, available=0, error='')
        try:
            info=mt5.symbol_info(symbol)
            if info is None:
                raise ValueError('Exact broker symbol unavailable')
            if not info.visible and not mt5.symbol_select(symbol,True):
                raise ValueError('Could not select symbol in Market Watch')
            rows=mt5.copy_rates_from_pos(symbol,getattr(mt5,'TIMEFRAME_'+job['timeframe']),1,WINDOW+1)
            if rows is None:
                raise ValueError('Broker history unavailable')
            if any(int(row['time'])+seconds>now.timestamp() for row in rows):
                raise ValueError('Unfinished or future candle rejected')
            item['returns']=aligned_returns(rows,times,seconds)
            item['available']=sum(v is not None for v in item['returns'])
        except (AttributeError,TypeError,ValueError,RuntimeError) as error:
            item['error']=str(error)[:300]
        series.append(item)
    positions=mt5.positions_get()
    exposure=[]
    for symbol in symbols:
        own=[] if positions is None else [p for p in positions if p.symbol==symbol and 410600<=getattr(p,'magic',0)<=410614]
        exposure.append(dict(symbol=symbol,known=positions is not None,
            gross_lots=sum(float(p.volume) for p in own),
            net_lots=sum(float(p.volume)*(1 if p.type==mt5.POSITION_TYPE_BUY else -1) for p in own),positions=len(own)))
    if identity() != wanted:
        raise ValueError('Account changed during correlation read')
    pairs=[dict(left=a['symbol'],right=b['symbol'],**pair_statistics(a['returns'],b['returns'])) for a,b in combinations(series,2)]
    return dict(login=job['login'],server=job['server'],currency=job['currency'],symbol=job['symbol'],
        revision=job['revision'],timeframe=job['timeframe'],captured_ms=int(now.timestamp()*1000),
        window=WINDOW,min_pairs=MIN_PAIRS,times=times,series=series,pairs=pairs,exposure=exposure,
        note='Per-symbol lots are not currency-normalized exposure. No filling of missing data and no execution effect.')
