"""MT5 snapshots and explicitly selected independent-brain broker execution.

Sniper mode has two compatible fast paths.  The packaged MQL5 bridge can wake
this process directly from MT5 OnTick through an authenticated local named pipe;
when that bridge is absent or stale, the proven 5 ms Python quote polling path
remains as a fallback.  Account reconciliation, deal history, DOM, candle
history and research collection stay on slower bounded schedules.

Broker execution remains in execution.py.  The MQL5 bridge transports ticks
only and never owns order submission.
"""
from __future__ import annotations
import json
import math
import os
from pathlib import Path
import time
import urllib.request
from datetime import datetime, timedelta, timezone

from sniper_native import NativeTickBridge

BASE = "http://127.0.0.1:18741"
BAGHDAD = timezone(timedelta(hours=3))
SNIPER_LOOP_SECONDS = 0.005
SNIPER_FULL_SNAPSHOT_SECONDS = 0.250
SNIPER_BACKGROUND_SECONDS = 0.500
CONFIG_REFRESH_SECONDS = 0.500
NORMAL_MIN_LOOP_SECONDS = 0.050
NATIVE_RECENT_MS = 500
NATIVE_BATCH_MAX = 256


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
                  account_trade_mode=int(account.trade_mode),margin_mode=getattr(account,"margin_mode",None),
                  trade_allowed=bool(getattr(account,"trade_allowed",False) and getattr(terminal,"trade_allowed",False) and not getattr(terminal,"tradeapi_disabled",False)),trade_expert=bool(getattr(account,"trade_expert",False)),
                  balance=account.balance, equity=account.equity,
                  positions=[dict(ticket=p.ticket, symbol=p.symbol,magic=getattr(p,"magic",0),identifier=getattr(p,"identifier",p.ticket),opened_ms=getattr(p,"time_msc",0),
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


def sniper_enabled(config: dict) -> bool:
    trading = config.get('trading') or {}
    return str(trading.get('mode', '')).strip().lower() == 'sniper'


def refresh_fast_tick(snapshot: dict, mt5, symbol: str, now_ms: int | None = None):
    """Overlay only the newest observable MT5 quote onto a reconciled snapshot."""
    if not snapshot or not snapshot.get('connected'):
        return snapshot, False, None
    tick = mt5.symbol_info_tick(symbol)
    if tick is None:
        return snapshot, False, None
    bid=float(getattr(tick,'bid',0.0)); ask=float(getattr(tick,'ask',0.0)); tms=int(getattr(tick,'time_msc',0) or 0)
    if not all(math.isfinite(v) and v>0 for v in (bid,ask)) or ask < bid or tms <= 0:
        return snapshot, False, None
    now_ms = int(time.time()*1000) if now_ms is None else int(now_ms)
    if tms > now_ms + 2000:
        return snapshot, False, None
    signature=(tms,bid,ask)
    previous=(snapshot.get('quote_time_ms'),snapshot.get('bid'),snapshot.get('ask'))
    updated=dict(snapshot)
    updated.update(bid=bid,ask=ask,quote_time_ms=tms,tick_age_ms=max(0,now_ms-tms))
    return updated, signature != previous, signature


def apply_native_tick(snapshot: dict, packet: dict, now_ms: int | None = None):
    """Overlay one authenticated EA OnTick packet without touching reconciled state."""
    if not snapshot or not snapshot.get('connected'):
        return snapshot, False, None
    now_ms = int(time.time()*1000) if now_ms is None else int(now_ms)
    try:
        login=int(packet['login']);symbol=str(packet['symbol']);tms=int(packet['time_msc'])
        bid=float(packet['bid']);ask=float(packet['ask'])
    except (KeyError,TypeError,ValueError,OverflowError):
        return snapshot, False, None
    if login != int(snapshot.get('login') or 0) or symbol != snapshot.get('symbol'):
        return snapshot, False, None
    if tms <= 0 or tms > now_ms + 2000 or now_ms - tms > 1000:
        return snapshot, False, None
    if not all(math.isfinite(value) and value > 0 for value in (bid,ask)) or ask < bid:
        return snapshot, False, None
    signature=(tms,bid,ask)
    previous=(snapshot.get('quote_time_ms'),snapshot.get('bid'),snapshot.get('ask'))
    updated=dict(snapshot)
    updated.update(bid=bid,ask=ask,quote_time_ms=tms,tick_age_ms=max(0,now_ms-tms))
    return updated, signature != previous, signature


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
            raise ValueError('Invalid, unfinished or out-of-range historical candles')
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
        return
    config_path = directory / "config.json"
    runtime_config = json.loads(config_path.read_text("utf-8")) if config_path.exists() else {}
    path = runtime_config.get("mt5_path", "")
    symbol = runtime_config.get("symbol", "XAUUSD")
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
    native=NativeTickBridge()
    latest_snapshot=None
    next_full_snapshot=0.0
    next_background=0.0
    next_config_refresh=0.0
    normal_poll_seconds=1.0

    def api(path, payload=None):
        request = urllib.request.Request(BASE+path,
            data=json.dumps(payload, allow_nan=False).encode('utf-8') if payload is not None else None,
            headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'})
        with opener.open(request,timeout=4) as response:
            return json.load(response)

    def publish(snapshot):
        body = json.dumps(snapshot, allow_nan=False).encode("utf-8")
        request = urllib.request.Request(BASE + "/v1/gateway/snapshot", data=body,
                  headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
        with opener.open(request, timeout=4) as response:
            response.read()

    def background_work():
        try:
            job = api('/v1/gateway/history/request').get('job')
            if job is not None:
                api('/v1/gateway/history/chunk', collect_history_chunk(mt5, job))
                return
            from studio_feed import collect_frame
            request=api('/v1/gateway/research/request')
            if request.get('job') is not None:
                api('/v1/gateway/research/frame',collect_frame(mt5,request['job']))
        except (OSError, ValueError):
            pass

    from execution import BrokerExecutor
    executor=BrokerExecutor(directory / 'broker-journal.sqlite')
    try:
        while True:
            loop_started=time.perf_counter()
            mono=time.monotonic()
            command_polled=False
            if mono >= next_config_refresh:
                next_config_refresh=mono+CONFIG_REFRESH_SECONDS
                try:
                    if config_path.exists():
                        candidate=json.loads(config_path.read_text("utf-8"))
                        if isinstance(candidate,dict):
                            runtime_config=candidate
                except (OSError,ValueError,TypeError):
                    pass
            sniper=sniper_enabled(runtime_config)

            if mt5 is None:
                latest_snapshot = empty_snapshot("MT5 bridge dependency unavailable. Use the complete Windows package.")
                if mono >= next_full_snapshot:
                    next_full_snapshot=mono+1.0
                    try:
                        publish(latest_snapshot);core_failures=0
                    except (OSError,ValueError):
                        core_failures+=1
            else:
                try:
                    if not initialized and mono >= retry_at:
                        initialized = mt5.initialize(path, timeout=5000) if path else mt5.initialize(timeout=5000)
                        retry_at = mono + 5
                        next_full_snapshot=0.0
                        if initialized:
                            native.bind_terminal(mt5.terminal_info())
                    if initialized:
                        # Drain every authenticated OnTick packet first.  Each one is
                        # published and followed immediately by command pickup, so a
                        # queued burst is not collapsed into one synthetic latest tick.
                        if sniper and latest_snapshot and latest_snapshot.get('connected'):
                            packets=native.drain(int(latest_snapshot.get('login') or 0),symbol,NATIVE_BATCH_MAX)
                            for packet in packets:
                                native_snapshot,changed,_=apply_native_tick(latest_snapshot,packet)
                                if not changed:
                                    continue
                                latest_snapshot=native_snapshot
                                publish(latest_snapshot)
                                core_failures=0
                                try:
                                    normal_poll_seconds=max(NORMAL_MIN_LOOP_SECONDS,min(1.0,executor.cycle(mt5,api)))
                                    command_polled=True
                                except (OSError,ValueError,RuntimeError):
                                    pass

                        full_interval=SNIPER_FULL_SNAPSHOT_SECONDS if sniper else max(0.5,min(1.0,normal_poll_seconds))
                        full_due=latest_snapshot is None or mono >= next_full_snapshot
                        if full_due:
                            latest_snapshot=collect(mt5,symbol,market_cache=market_cache,dom_cache=dom_cache)
                            next_full_snapshot=mono+full_interval
                            publish(latest_snapshot)
                            core_failures=0
                        elif sniper and latest_snapshot.get('connected') and not native.recent(maximum_age_ms=NATIVE_RECENT_MS):
                            # Native EA is optional.  If it is absent/stale the exact
                            # v0.7 5 ms polling path remains active as a fallback.
                            fast_snapshot,changed,_=refresh_fast_tick(latest_snapshot,mt5,symbol)
                            if changed:
                                latest_snapshot=fast_snapshot
                                publish(latest_snapshot)
                                core_failures=0
                        if initialized and latest_snapshot and not latest_snapshot.get('connected'):
                            dom_cache.release(mt5);mt5.shutdown();initialized=False
                    elif mono >= next_full_snapshot:
                        latest_snapshot=empty_snapshot("MT5 connection failed. Set terminal64.exe path and sign in inside MT5.")
                        next_full_snapshot=mono+1.0
                        publish(latest_snapshot)
                except Exception:
                    latest_snapshot=empty_snapshot("MT5 read failed; retrying connection.")
                    dom_cache.release(mt5)
                    try:
                        mt5.shutdown()
                    except Exception:
                        pass
                    initialized=False
                    retry_at=time.monotonic()+5
                    next_full_snapshot=0.0

            try:
                if initialized and latest_snapshot and latest_snapshot.get('connected'):
                    if not command_polled:
                        try:
                            normal_poll_seconds=max(NORMAL_MIN_LOOP_SECONDS,min(1.0,executor.cycle(mt5,api)))
                        except (OSError,ValueError,RuntimeError):
                            pass
                    mono=time.monotonic()
                    background_interval=SNIPER_BACKGROUND_SECONDS if sniper else max(0.25,normal_poll_seconds)
                    if mono >= next_background:
                        next_background=mono+background_interval
                        background_work()
                if core_failures >= 3:
                    return
            except (OSError,ValueError):
                core_failures+=1
                if core_failures >= 3:
                    return

            target=SNIPER_LOOP_SECONDS if sniper and initialized else normal_poll_seconds
            elapsed=time.perf_counter()-loop_started
            if target > elapsed:
                remaining=target-elapsed
                if sniper and initialized and native.available:
                    # Named-pipe OnTick wakes this wait immediately; 5 ms is only
                    # the maximum fallback polling interval, not a mandatory sleep.
                    native.wait(remaining)
                else:
                    time.sleep(remaining)
    finally:
        native.close()
        executor.close()
        if mt5 is not None:
            dom_cache.release(mt5)
            mt5.shutdown()
        lock.close()


if __name__ == "__main__":
    main()
