"""Read-only MT5 bootstrap gateway. Never submits, modifies or closes orders.

The initial gateway polls account snapshots. It is not the future tick/FLASH
transport; that requires a separately validated native/MQL5 adapter.
"""
from __future__ import annotations
import json
import math
import os
from pathlib import Path
import sys
import time
import urllib.request
from datetime import datetime, timedelta, timezone

BASE = "http://127.0.0.1:18741"
BAGHDAD = timezone(timedelta(hours=3))

class MarketCache:
    def __init__(self):
        self.key=None; self.data=None; self.next_read=0.0
    def read(self,mt5,key,now):
        if self.key!=key or time.monotonic()>=self.next_read:
            self.key=key;self.next_read=time.monotonic()+5.0
            self.data=collect_market(mt5,key[2],now)
        return self.data

class DomCache:
    """Optional MT5 Market Book subscription. It never fabricates depth."""
    def __init__(self):
        self.key=None
        self.active=False
        self.next_retry=0.0
    def release(self,mt5):
        if self.active and self.key is not None:
            try:
                mt5.market_book_release(self.key[2])
            except Exception:
                pass
        self.key=None
        self.active=False
        self.next_retry=0.0
    def read(self,mt5,key):
        if self.key!=key:
            self.release(mt5)
            self.key=key
        if not self.active and time.monotonic() >= self.next_retry:
            self.next_retry=time.monotonic()+30.0
            try:
                self.active=bool(mt5.market_book_add(key[2]))
            except Exception:
                self.active=False
        if not self.active:
            return False,[]
        try:
            book=mt5.market_book_get(key[2])
            if not book:
                return False,[]
            buy_types={getattr(mt5,'BOOK_TYPE_BUY',2)}
            sell_types={getattr(mt5,'BOOK_TYPE_SELL',1)}
            rows=[]
            for level in book:
                kind=getattr(level,'type',None)
                side='buy' if kind in buy_types else 'sell' if kind in sell_types else None
                if side is None:
                    continue
                price=float(getattr(level,'price',0.0))
                real=float(getattr(level,'volume_real',0.0) or 0.0)
                volume=real if real>0 else float(getattr(level,'volume',0.0) or 0.0)
                if math.isfinite(price) and math.isfinite(volume) and price>0 and volume>0:
                    rows.append(dict(side=side,price=price,volume=volume))
            rows.sort(key=lambda row:(0 if row['side']=='buy' else 1,-row['price'] if row['side']=='buy' else row['price']))
            # Preserve both sides even when one side has more than 100 rows.
            # Multiple rows at one price are one level for nearest-depth analysis.
            levels={}
            for row in rows:
                identity=(row['side'],row['price'])
                levels[identity]=levels.get(identity,0.0)+row['volume']
            bids=[dict(side=side,price=price,volume=volume) for (side,price),volume in levels.items() if side=='buy' and math.isfinite(volume)]
            asks=[dict(side=side,price=price,volume=volume) for (side,price),volume in levels.items() if side=='sell' and math.isfinite(volume)]
            bids.sort(key=lambda row:-row['price']);asks.sort(key=lambda row:row['price'])
            depth=bids[:50]+asks[:50]
            return bool(bids and asks),depth
        except Exception:
            return False,[]

def collect_market(mt5,symbol,now):
    data=dict(captured_ms=int(now.timestamp()*1000),m1=[],m5=[],m15=[],error="")
    errors=[]
    for name,constant,seconds in (("m1","TIMEFRAME_M1",60),("m5","TIMEFRAME_M5",300),("m15","TIMEFRAME_M15",900)):
        try:
            rates=mt5.copy_rates_from_pos(symbol,getattr(mt5,constant),1,180)
            if rates is None or len(rates)<100:
                errors.append(name.upper()+": waiting for 100 closed candles");continue
            rows=[dict(time=int(r['time']),open=float(r['open']),high=float(r['high']),low=float(r['low']),close=float(r['close']),tick_volume=int(r['tick_volume'])) for r in rates]
            if any(r['time']+seconds>now.timestamp() for r in rows):
                errors.append(name.upper()+": unfinished or future candle rejected");continue
            data[name]=rows
        except (AttributeError,TypeError,ValueError,RuntimeError):
            errors.append(name.upper()+": history unavailable")
    data['error']='; '.join(errors)
    return data


def empty_snapshot(error: str) -> dict:
    return dict(connected=False, error=error, login=0, server="", currency="",
                demo=False, balance=0.0, equity=0.0, positions=[], pending_orders=0,
                today_net=None, today_deals=None, report_day="", bid=None, ask=None,
                tick_age_ms=None, symbol="", volume_min=None, volume_step=None,
                tick_size=None, dom_available=False,dom=[],volume_max=None,tick_value_loss=None,tick_value_profit=None,market=None,point=None)


def day_range(now: datetime):
    local = now.astimezone(BAGHDAD)
    start = local.replace(hour=0, minute=0, second=0, microsecond=0)
    return start.astimezone(timezone.utc), now.astimezone(timezone.utc), local.date().isoformat()


def deal_summary(deals, mt5):
    if deals is None:
        return None, None
    trading = [d for d in deals if d.type in (mt5.DEAL_TYPE_BUY, mt5.DEAL_TYPE_SELL)]
    net = sum(d.profit + d.commission + d.swap + getattr(d, "fee", 0.0) for d in trading)
    return net, len(trading)


def collect(mt5, symbol: str, now: datetime | None = None, market_cache=None, dom_cache=None) -> dict:
    now = now or datetime.now(timezone.utc)
    terminal = mt5.terminal_info()
    account = mt5.account_info()
    if terminal is None or not terminal.connected or account is None:
        return empty_snapshot("MT5 disconnected or account is not logged in. Sign in inside MT5.")
    positions = mt5.positions_get()
    orders = mt5.orders_get()
    if positions is None or orders is None:
        return empty_snapshot("MT5 position/order reconciliation failed; account snapshot withheld.")
    start, end, report_day = day_range(now)
    deals = mt5.history_deals_get(start, end)
    net, count = deal_summary(deals, mt5)
    info = mt5.symbol_info(symbol)
    if info is not None and not info.visible:
        mt5.symbol_select(symbol, True)
        info = mt5.symbol_info(symbol)
    tick = mt5.symbol_info_tick(symbol) if info is not None else None
    key=(account.login,account.server,symbol)
    market=market_cache.read(mt5,key,now) if market_cache is not None and info is not None else None
    dom_available,dom=dom_cache.read(mt5,key) if dom_cache is not None and info is not None else (False,[])
    # Re-read account identity after collection to reject account-switch races.
    current = mt5.account_info()
    if current is None or (current.login, current.server) != (account.login, account.server):
        return empty_snapshot("Account changed during snapshot; waiting for reconciliation.")
    errors = []
    if info is None:
        errors.append(f"Symbol {symbol} is unavailable; choose its exact broker name in Settings.")
    if deals is None:
        errors.append("Deal history unavailable; daily PnL is unknown.")
    result = empty_snapshot(" ".join(errors))
    result.update(connected=True, login=account.login, server=account.server,
                  currency=account.currency, demo=account.trade_mode == mt5.ACCOUNT_TRADE_MODE_DEMO,
                  balance=account.balance, equity=account.equity,
                  positions=[dict(ticket=p.ticket, symbol=p.symbol,
                                  side="BUY" if p.type == mt5.POSITION_TYPE_BUY else "SELL",
                                  volume=p.volume, price_open=p.price_open,
                                  price_current=p.price_current, profit=p.profit, sl=p.sl, tp=p.tp)
                             for p in positions], pending_orders=len(orders), today_net=net,
                  today_deals=count, report_day=report_day, symbol=symbol,
                  bid=tick.bid if tick else None, ask=tick.ask if tick else None,
                  quote_time_ms=int(tick.time_msc) if tick else None,
                  tick_age_ms=max(0, int(now.timestamp() * 1000) - tick.time_msc) if tick and tick.time_msc<=int(now.timestamp()*1000)+2000 else None,
                  volume_min=info.volume_min if info else None,
                  volume_step=info.volume_step if info else None,
                  tick_size=info.trade_tick_size if info else None,dom_available=dom_available,dom=dom,
                  volume_max=getattr(info,'volume_max',None),tick_value_loss=getattr(info,'trade_tick_value_loss',None),tick_value_profit=getattr(info,'trade_tick_value_profit',None),market=market,point=getattr(info,'point',None))
    return result


def collect_history_chunk(mt5, job, now=None):
    """One bounded UTC day of M1 history. No invented bars or account mixing."""
    now = now or datetime.now(timezone.utc)
    result = {key: job[key] for key in ('id', 'from', 'to', 'login', 'server', 'symbol', 'currency', 'point')}
    result.update(bars=[], error='')
    try:
        start, end = int(job['from']), int(job['to'])
        if start <= 0 or end <= start or end-start > 86400 or end > now.timestamp():
            raise ValueError('Invalid UTC history interval')
        def identity(account):
            return (account.login, account.server, account.currency) if account else None
        expected = (job['login'], job['server'], job['currency'])
        if identity(mt5.account_info()) != expected:
            raise ValueError('Account changed before historical read')
        info = mt5.symbol_info(job['symbol'])
        if info is None or not math.isfinite(info.point) or abs(info.point-job['point']) > job['point']*1e-8:
            raise ValueError('Symbol point specification changed')
        rates = mt5.copy_rates_range(job['symbol'], mt5.TIMEFRAME_M1,
            datetime.fromtimestamp(start, timezone.utc), datetime.fromtimestamp(end-1, timezone.utc))
        if rates is None:
            raise ValueError('MT5 history unavailable; check chart history and Max bars in chart')
        if len(rates) > 1440:
            raise ValueError('Historical batch exceeds one UTC day')
        rows = [dict(time=int(r['time']), open=float(r['open']), high=float(r['high']),
            low=float(r['low']), close=float(r['close']), tick_volume=int(r['tick_volume']),
            spread_points=int(r['spread'])) for r in rates]
        if any(r['time'] < start or r['time']+60 > end or r['time'] % 60 or
               r['spread_points'] < 0 or any(not math.isfinite(r[k]) for k in ('open','high','low','close')) for r in rows):
            raise ValueError('Invalid, unfinished or out-of-range historical candle')
        if any(a['time'] >= b['time'] for a,b in zip(rows,rows[1:])):
            raise ValueError('Duplicate or unordered historical candles')
        if identity(mt5.account_info()) != expected:
            raise ValueError('Account changed during historical read')
        result['bars'] = rows
    except (AttributeError, TypeError, ValueError, KeyError, RuntimeError, OverflowError) as error:
        result['error'] = str(error)[:300]
    return result


def lock_gateway(directory: Path):
    handle = (directory / "gateway.lock").open("a+b")
    handle.seek(0)
    if os.name == "nt":
        import msvcrt
        if not handle.read(1):
            handle.write(b"0")
            handle.flush()
        handle.seek(0)
        msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
    else:
        import fcntl
        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
    return handle


def main():
    directory = Path(os.environ["BRAINTRADE_DATA_DIR"])
    token = os.environ.pop("BRAINTRADE_IPC_TOKEN")
    try:
        lock = lock_gateway(directory)
    except OSError:
        return  # Existing gateway will exit if its parent core becomes unavailable.
    config_path = directory / "config.json"
    config = json.loads(config_path.read_text("utf-8")) if config_path.exists() else {}
    path = config.get("mt5_path", "")
    symbol = config.get("symbol", "XAUUSD")
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    try:
        import MetaTrader5 as mt5
    except ImportError:
        mt5 = None
    initialized = False
    retry_at = 0.0
    core_failures = 0
    market_cache=MarketCache()
    dom_cache=DomCache()
    def api(path, payload=None):
        request = urllib.request.Request(BASE+path,
            data=json.dumps(payload, allow_nan=False).encode('utf-8') if payload is not None else None,
            headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'})
        with opener.open(request,timeout=4) as response:
            return json.load(response)
    try:
        while True:
            if mt5 is None:
                snapshot = empty_snapshot("MT5 bridge dependency unavailable. Use the complete Windows package.")
            else:
                try:
                    if not initialized and time.monotonic() >= retry_at:
                        # Official initialize() starts the terminal if necessary.
                        # Saved terminal login is used; no password is held here.
                        initialized = mt5.initialize(path, timeout=5000) if path else mt5.initialize(timeout=5000)
                        retry_at = time.monotonic() + 5
                    snapshot = collect(mt5, symbol,market_cache=market_cache,dom_cache=dom_cache) if initialized else empty_snapshot(
                        "MT5 connection failed. Set terminal64.exe path and sign in inside MT5.")
                    if initialized and not snapshot["connected"]:
                        dom_cache.release(mt5)
                        mt5.shutdown()
                        initialized = False
                except Exception:
                    snapshot = empty_snapshot("MT5 read failed; retrying connection.")
                    dom_cache.release(mt5)
                    mt5.shutdown()
                    initialized = False
                    retry_at = time.monotonic() + 5
            try:
                body = json.dumps(snapshot, allow_nan=False).encode("utf-8")
                request = urllib.request.Request(BASE + "/v1/gateway/snapshot", data=body,
                          headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
                with opener.open(request, timeout=4) as response:
                    response.read()
                core_failures = 0
                poll_seconds = 1.0
                if initialized and snapshot['connected']:
                    try:
                        job = api('/v1/gateway/history/request').get('job')
                        if job is not None:
                            api('/v1/gateway/history/chunk', collect_history_chunk(mt5, job))
                        else:
                            from studio_feed import collect_frame
                            request=api('/v1/gateway/research/request')
                            poll_seconds=max(0.25,min(1.0,request.get('poll_ms',1000)/1000.0))
                            if request.get('job') is not None:
                                api('/v1/gateway/research/frame',collect_frame(mt5,request['job']))
                    except (OSError, ValueError):
                        pass  # Live snapshot succeeded; an obsolete history job may have been cancelled.
            except (OSError, ValueError):
                core_failures += 1
                if core_failures >= 3:
                    return
            time.sleep(locals().get("poll_seconds",1.0))
    finally:
        if mt5 is not None:
            dom_cache.release(mt5)
            mt5.shutdown()
        lock.close()


if __name__ == "__main__":
    main()
