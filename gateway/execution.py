"""MT5 broker adapter with durable no-duplicate dispatch semantics.

Every send is journaled durably before crossing the MT5 API. An uncertain send
is reconciled, never blindly retried. Sniper tick entries use a measured fast
path that removes the broker ``order_check`` round trip after equivalent local
contract validation, while retaining account binding, ownership checks,
authorization and the durable pre-send journal.

The Sniper pulse path may prefetch Core work in the same quote request. Reusing
that work removes one localhost request from the critical path; final Core
authorization and the durable send journal remain mandatory before order_send.
"""
from __future__ import annotations
from datetime import datetime, timezone, timedelta
import json
import math
import sqlite3
import time

MAGIC_MIN, MAGIC_MAX = 410600, 410617
# Reserved 410616 belongs to the unmerged Adaptive Learner, not this build.
OWNED_MAGICS = frozenset(range(410600, 410616)) | {410617}
FAST_QUEUE_TARGET_MS = 20


def clock_ms():
    return int(time.time() * 1000)


def account_identity(a):
    return (a.login, a.server, a.currency) if a else None


def expected(cmd):
    return cmd['login'], cmd['server'], cmd['currency']


def values(obj):
    return obj._asdict() if hasattr(obj, '_asdict') else vars(obj)


def is_fast_path(cmd):
    """Return True only for latency-sensitive market actions.

    Tick-keyed entries are the Sniper path. Closing an already-owned position is
    also safe to dispatch without a separate order_check round trip because the
    exact broker position is re-read and validated immediately before send.
    """
    if cmd.get('kind') == 'close':
        return True
    signal = cmd.get('signal') or {}
    return cmd.get('kind') == 'open' and str(signal.get('key', '')).startswith('tick:')


def sniper_broker_geometry(info, tick, stop, target):
    """Validate a frozen Sniper TP without ever increasing its user pip target.

    Both distances are from executable entry. The broker validates SL/TP
    against the opposite-side exit quote. A full tick cushions tick rounding.
    If the broker's stops floor is too wide for the requested 5/10 pip TP,
    reject the entry with an exact explanation instead of silently raising TP
    to 40/50 pips. Likewise, never silently expand the selected stop loss.
    """
    step = float(info.trade_tick_size)
    spread = float(tick.ask) - float(tick.bid)
    minimum = max(0, getattr(info, 'trade_stops_level', 0)) * float(info.point) + step
    stop, target = float(stop), float(target)
    if not all(math.isfinite(v) and v > 0 for v in (stop, target, spread + step, minimum)):
        raise ValueError('Invalid Sniper broker geometry')
    # Frozen Sniper targets must be exactly representable on this broker's
    # actual tick grid. The later price-normalization ceil/floor would
    # otherwise silently extend a 5-pip request by up to one tick.
    tick_steps = target / step
    if abs(tick_steps - round(tick_steps)) > 1e-6:
        raise ValueError(
            f'Sniper exact pip TP {target:.5f} is not a multiple of broker tick size {step:.5f}; '
            'TP was NOT rounded or widened. Choose a tick-representable target.'
        )
    min_stop = spread + minimum
    min_target = max(step, minimum - spread)
    if stop + step * 1e-7 < min_stop:
        raise ValueError(f'Sniper SL too close for broker stop level: requested {stop:.5f}, minimum {min_stop:.5f} in price. SL was not widened.')
    if target + step * 1e-7 < min_target:
        raise ValueError(f'Sniper exact pip TP cannot satisfy broker stop level: requested {target:.5f}, minimum {min_target:.5f} in price. TP was NOT widened.')
    return stop, target, minimum, spread


def round_trip_commission(rate_per_lot, lots):
    """Estimated broker fee in account currency for entry AND exit, never a broker charge."""
    rate, volume = float(rate_per_lot), float(lots)
    if not math.isfinite(rate) or not 0 <= rate <= 1000 or not math.isfinite(volume) or not 0 < volume <= 100:
        raise ValueError('Commission estimate requires non-negative round-trip rate and positive lot size')
    return rate * volume


def size_to_stop_risk(mt5, account, info, symbol, side, entry, stop_price,
                      requested, risk_percent, commission, slippage_ticks,
                      micro_demo_risk_opt_in=False, broker_demo=False,
                      micro_real_active=False):
    """Limit order size to an equity-at-risk budget using MT5 contract PnL.

    A requested volume is an UPPER bound, never a minimum. No order can be
    increased or sent without reliable broker order_calc_profit. The cost
    budget includes full-roundtrip configured commission and a conservative
    two-side slippage estimate. This does not guarantee a stop fills at its
    stop price in a volatile or gapping market.
    """
    values=(entry, stop_price, requested, risk_percent, commission, slippage_ticks)
    if not all(math.isfinite(float(v)) for v in values):
        raise ValueError('Risk sizing: all prices, lot and fee inputs must be finite')
    allowed_risk_percent = (20.0 if micro_real_active
                            else 5.0 if micro_demo_risk_opt_in and broker_demo
                            else 2.0)
    if not (entry > 0 and stop_price > 0 and requested > 0
            and 0 < risk_percent <= allowed_risk_percent
            and 0 <= commission <= 1000
            and 0 <= slippage_ticks <= 1000):
        raise ValueError('Risk sizing: invalid price, lot, risk percent or fee')
    equity=float(getattr(account,'equity',float('nan')))
    if not math.isfinite(equity) or equity <= 0:
        raise ValueError('Risk sizing: fresh positive broker equity required')
    if (side==1 and stop_price>=entry) or (side==-1 and stop_price<=entry):
        raise ValueError('Risk sizing: broker stop must represent a real downside loss')
    if side not in (-1,1):
        raise ValueError('Risk sizing: invalid trade direction')
    step=float(info.volume_step)
    low=math.ceil(float(info.volume_min)/step-1e-9)
    high=math.floor(min(requested,float(info.volume_max))/step+1e-9)
    if low>high:
        raise ValueError('Risk sizing: requested lot is below broker minimum; order NOT enlarged')
    # Use the broker's own monetary profit calculator rather than assuming
    # XAUUSD always has a particular USD-per-tick or contract size.
    ref=round(high*step,10)
    order_type=mt5.ORDER_TYPE_BUY if side==1 else mt5.ORDER_TYPE_SELL
    try:
        pnl=mt5.order_calc_profit(order_type,symbol,ref,entry,stop_price)
    except Exception as exc:
        raise ValueError('Risk sizing: broker profit estimate unavailable') from exc
    if pnl is None or not math.isfinite(float(pnl)) or float(pnl)>=0:
        raise ValueError('Risk sizing: broker loss estimate unavailable or inconsistent')
    loss_per_lot=-float(pnl)/ref
    if slippage_ticks:
        tick_loss=float(getattr(info,'trade_tick_value_loss',float('nan')))
        if not math.isfinite(tick_loss) or tick_loss<=0:
            raise ValueError('Risk sizing: broker loss-side tick value unavailable for slippage')
        slip_per_lot=2.0*slippage_ticks*tick_loss
    else:
        slip_per_lot=0.0
    unit_risk=loss_per_lot+commission+slip_per_lot
    if not math.isfinite(unit_risk) or unit_risk<=0:
        raise ValueError('Risk sizing: invalid total broker stop risk per lot')
    budget=equity*risk_percent/100.0
    allowed=math.floor((budget/unit_risk)/step+1e-9)
    units=min(high,allowed)
    if units<low:
        minimum_risk = unit_risk * low * step
        required_percent = minimum_risk / equity * 100.0
        raise ValueError(
            f'Risk sizing: minimum {low*step:g} lot needs '
            f'{minimum_risk:.4f} {account.currency} at SL '
            f'({required_percent:.3f}% of equity), '
            f'budget {budget:.4f} ({risk_percent:.3f}%); entry rejected, '
            'never increased or widened. Leverage changes margin, not stop loss.')
    volume=round(units*step,10)
    actual=unit_risk*volume
    if actual>budget+max(1e-8,budget*1e-10):
        raise ValueError('Risk sizing: projected stop loss exceeds equity budget')
    return volume,dict(policy='mt5_stop_risk_v1',
                       requested=requested,sent=volume,
                       broker_equity=equity,risk_percent=risk_percent,
                       risk_budget=budget,broker_stop_loss_per_lot=loss_per_lot,
                       commission_round_trip_per_lot=commission,
                       slippage_cushion_per_lot=slip_per_lot,
                       estimated_stop_loss_including_costs=actual,
                       reduced=volume<requested-1e-9,
                       source='MT5 order_calc_profit; stop price is not a guaranteed fill')



def affordable_demo_volume(mt5, account, info, symbol, side, price, requested, commission):
    """Round down and shrink only; broker minimum and free margin remain real constraints.

    No retries after order_send. order_calc_margin is a pre-send estimate, so the
    broker still owns final acceptance when its margin calculation differs.
    """
    step = float(info.volume_step)
    free = float(getattr(account, 'margin_free', float('nan')))
    if not all(math.isfinite(v) for v in (requested, commission, free)) or requested <= 0 or commission < 0 or free < 0:
        raise ValueError('Demo sizing: valid free margin, volume and commission required')
    low = math.ceil(info.volume_min / step - 1e-9)
    high = math.floor(min(requested, info.volume_max) / step + 1e-9)
    if low > high:
        raise ValueError(f'Demo sizing: requested {requested:g} lot is below broker minimum {info.volume_min:g}; volume was not increased')
    order_type = mt5.ORDER_TYPE_BUY if side == 1 else mt5.ORDER_TYPE_SELL
    def estimate(units):
        volume = round(units * step, 10)
        margin = mt5.order_calc_margin(order_type, symbol, volume, price)
        if margin is None or not math.isfinite(margin) or margin < 0:
            raise ValueError('Demo sizing: broker margin estimate unavailable')
        return volume, float(margin), float(margin) + round_trip_commission(commission, volume)
    chosen = estimate(high)
    if chosen[2] > free:
        minimum = estimate(low)
        if minimum[2] > free:
            raise ValueError(f'Demo sizing: broker minimum {minimum[0]:g} lot needs {minimum[2]:.2f} {account.currency} including configured commission; free margin {free:.2f}')
        chosen = minimum
        lo, hi = low + 1, high - 1
        while lo <= hi:
            mid = (lo + hi) // 2
            candidate = estimate(mid)
            if candidate[2] <= free:
                chosen = candidate
                lo = mid + 1
            else:
                hi = mid - 1
    volume, margin, total = chosen
    return volume, dict(policy='demo_downsize_only_v1', requested=requested, sent=volume,
                        free_margin=free, estimated_margin=margin, estimated_margin_plus_commission=total,
                        commission_round_trip_per_lot=commission,
                        estimated_round_trip_commission=round_trip_commission(commission,volume),
                        broker_minimum=info.volume_min, reduced=volume < requested - 1e-9)


class BrokerExecutor:
    def __init__(self, path):
        self.db = sqlite3.connect(path)
        self.db.execute('PRAGMA journal_mode=WAL')
        # FULL is intentional: the no-duplicate send journal must reach durable
        # storage before order_send. We measure this cost instead of weakening it.
        self.db.execute('PRAGMA synchronous=FULL')
        self.db.execute('PRAGMA busy_timeout=50')
        self.db.execute('CREATE TABLE IF NOT EXISTS sends(id TEXT PRIMARY KEY, request TEXT NOT NULL, result TEXT, created_ms INTEGER NOT NULL)')
        self.db.execute('CREATE TABLE IF NOT EXISTS micro_real_recovered(account_key TEXT PRIMARY KEY, reached_ms INTEGER NOT NULL, reached_equity REAL NOT NULL)')
        self.db.execute('CREATE TABLE IF NOT EXISTS daily_loss_locks(account_key TEXT NOT NULL, report_day TEXT NOT NULL, reached_ms INTEGER NOT NULL, loss_amount REAL NOT NULL, PRIMARY KEY(account_key,report_day))')
        self.db.commit()
        self.next_history = 0.0

    def _daily_loss_blocked(self, mt5, cmd, account):
        """Daily equity circuit breaker across all symbols and external positions.

        A disabled switch removes only this gate, never stop/margin/identity
        checks. Existing daily lock is retained and applies if re-enabled.
        """
        enabled = cmd.get('daily_breaker_enabled',cmd.get('target')=='live')
        if enabled is not True:
            return False
        try:
            limit=float(cmd.get('daily_breaker_limit_percent',2.0))
            balance=float(account.balance)
            equity=float(account.equity)
        except (TypeError, ValueError, AttributeError):
            raise ValueError('Daily breaker: invalid broker account or limit')
        if not all(math.isfinite(v) for v in (limit,balance,equity)) or not 0.1<=limit<=10 or balance<=0 or equity<=0:
            raise ValueError('Daily breaker: valid balance, equity and percentage required')
        now=datetime.now(timezone.utc)
        local=now.astimezone(timezone(timedelta(hours=3)))
        day=local.date().isoformat()
        key=json.dumps((*account_identity(account),int(account.trade_mode)),ensure_ascii=False,separators=(',',':'))
        if self.db.execute('SELECT 1 FROM daily_loss_locks WHERE account_key=? AND report_day=?',(key,day)).fetchone():
            return True
        day_begin=local.replace(hour=0,minute=0,second=0,microsecond=0).astimezone(timezone.utc)
        try:
            deals=mt5.history_deals_get(day_begin,now)
            positions=mt5.positions_get()
        except Exception as exc:
            raise ValueError('Daily breaker: broker history/positions unavailable') from exc
        if deals is None or positions is None:
            raise ValueError('Daily breaker: broker history/positions unavailable')
        # Positions_get() without a symbol deliberately verifies all symbols.
        # Account equity includes all floating PnL, swaps and external positions.
        trading={mt5.DEAL_TYPE_BUY,mt5.DEAL_TYPE_SELL}
        net=0.0
        for d in deals:
            if d.type not in trading:
                continue
            value=sum(float(getattr(d,k,0.0)) for k in ('profit','commission','swap','fee'))
            if not math.isfinite(value):
                raise ValueError('Daily breaker: invalid broker deal costs')
            net+=value
        baseline=balance-net
        if not math.isfinite(baseline) or baseline<=0:
            raise ValueError('Daily breaker: invalid start-of-day trading balance estimate')
        loss=baseline-equity
        if loss>=baseline*limit/100.0:
            self.db.execute('INSERT OR IGNORE INTO daily_loss_locks VALUES(?,?,?,?)',
                            (key,day,clock_ms(),loss))
            self.db.commit()
            return True
        return False

    def _micro_real_active(self, cmd, account):
        """Verified REAL, explicit switch, and a one-way durable equity milestone.

        This only relaxes the *per-order equity risk budget*. Stop loss, lot,
        margin and duplicate protections must not be bypassed.
        """
        if cmd.get('micro_real_temporary') is not True:
            return False
        if cmd.get('target') != 'live' or getattr(account,'trade_mode',None)!=2:
            return False
        threshold=float(cmd.get('micro_real_recovery_equity',float('nan')))
        cap=float(cmd.get('risk_percent',float('nan')))
        equity=float(getattr(account,'equity',float('nan')))
        if not (math.isfinite(threshold) and 50<=threshold<=1_000_000
                and math.isfinite(cap) and 0<cap<=20
                and math.isfinite(equity) and equity>0):
            raise ValueError('Micro REAL: invalid broker equity, recovery threshold, or risk cap')
        key=json.dumps(account_identity(account),ensure_ascii=False,separators=(',',':'))
        if equity>=threshold:
            self.db.execute('INSERT OR IGNORE INTO micro_real_recovered VALUES(?,?,?)',
                            (key,clock_ms(),equity))
            self.db.commit()
        locked=self.db.execute('SELECT 1 FROM micro_real_recovered WHERE account_key=?',(key,)).fetchone()
        return equity<threshold and locked is None

    def close(self):
        self.db.close()

    def _save(self, cmd, result):
        self.db.execute('UPDATE sends SET result=? WHERE id=?', (json.dumps(result, allow_nan=False), cmd['id']))
        self.db.commit()
        return result

    def _reconcile(self, mt5, cmd, previous):
        result = previous or dict(id=cmd['id'], state='unknown', reason='Send was interrupted; no automatic retry')
        if result['state'] not in ('unknown', 'placed', 'partial'):
            return result
        # A journal is scoped to both broker identity and Demo/REAL mode.
        # A matching login tuple alone must never authorize cross-mode recovery.
        target_mode = 0 if cmd.get('target') == 'demo' else 2 if cmd.get('target') == 'live' else -1
        current_account = mt5.account_info()
        if (account_identity(current_account) != expected(cmd)
                or getattr(current_account, 'trade_mode', None) != target_mode):
            return result
        start = datetime.fromtimestamp(max(0, cmd['created_ms']-60000)/1000, timezone.utc)
        deals = mt5.history_deals_get(start, datetime.now(timezone.utc))
        orders = mt5.orders_get(symbol=cmd['symbol'])
        if deals is None or orders is None:
            return result
        tag = 'BT:' + cmd['id'][:20]
        # Broker comments are not guaranteed to survive a fill unchanged. A
        # positive MT5 order ticket from the durable acknowledgement is a
        # stronger exact link than the comment. Keep magic/symbol/account checks
        # and never guess based solely on timing or trade direction.
        acknowledged_order = int(result.get('order') or 0)
        def matches_identity(item):
            if getattr(item, 'magic', 0)!=cmd['magic'] or getattr(item, 'symbol', '')!=cmd['symbol']:
                return False
            item_order = int(getattr(item, 'order', 0) or 0)
            # If both sides expose an order ID, it must agree. A copied or
            # broker-reused comment must never override a conflicting ticket.
            if acknowledged_order>0 and item_order>0:
                return item_order==acknowledged_order
            return getattr(item, 'comment', '')==tag
        matches = [d for d in deals if matches_identity(d)]
        pending = any(matches_identity(o) for o in orders)
        # A journal is scoped to both broker identity and Demo/REAL mode.
        # A matching login tuple alone must never authorize cross-mode recovery.
        target_mode = 0 if cmd.get('target') == 'demo' else 2 if cmd.get('target') == 'live' else -1
        current_account = mt5.account_info()
        if (account_identity(current_account) != expected(cmd)
                or getattr(current_account, 'trade_mode', None) != target_mode):
            return result
        if matches and not pending:
            submitted = result.get('request') or cmd.get('broker_request')
            if not submitted:
                return dict(result, reason='Fill found but submitted volume unavailable; reconciliation remains pending')
            # MT5 deals represent individual immutable fills. Never invent
            # filled volume from corrupt history, duplicate deal IDs, or a
            # volume greater than the durable submitted request.
            try:
                tickets = [int(getattr(d, 'ticket', 0)) for d in matches]
                volumes = [float(getattr(d, 'volume', 0)) for d in matches]
                submitted_volume = float(submitted['volume'])
            except (TypeError, ValueError, KeyError, OverflowError):
                return dict(result, reason='Invalid broker history deal geometry; reconciliation pending')
            if any(t <= 0 for t in tickets) or len(set(tickets)) != len(tickets):
                return dict(result, reason='Duplicate or invalid broker deal tickets; reconciliation pending')
            if not (math.isfinite(submitted_volume) and submitted_volume > 0
                    and all(math.isfinite(v) and v > 0 for v in volumes)):
                return dict(result, reason='Invalid broker history deal volumes; reconciliation pending')
            volume = sum(volumes)
            if not math.isfinite(volume) or volume > submitted_volume + max(1e-8, submitted_volume * 1e-7):
                return dict(result, reason='Broker fill volume exceeds submitted volume; reconciliation pending')
            state = 'partial' if volume + 1e-8 < submitted_volume else 'filled'
            result = dict(result, state=state, reason='Broker history confirms fill; await Core position reconciliation; no resend',
                          request=submitted, deal=int(matches[0].ticket), deals=[int(d.ticket) for d in matches],
                          volume=volume, recovered_from_history=True)
            prices = [float(getattr(d, 'price', 0)) for d in matches]
            if all(math.isfinite(p) and p > 0 for p in prices):
                result['price'] = sum(p * float(d.volume) for p, d in zip(prices, matches)) / volume
            times = [int(getattr(d, 'time_msc', 0)) for d in matches]
            if all(t > 0 for t in times):
                result['fill_time_ms'] = min(times)
            positions = {int(getattr(d, 'position_id', 0)) for d in matches}
            if len(positions) == 1 and next(iter(positions)) > 0:
                result['position_id'] = next(iter(positions))
            # Recovery can restore booked evidence, never the unobserved quote path.
            result['reconciled_ms'] = clock_ms()
            return self._save(cmd, result)
        return result

    def execute(self, mt5, cmd, authorize):
        row = self.db.execute('SELECT request,result FROM sends WHERE id=?', (cmd['id'],)).fetchone()
        if row:
            saved = json.loads(row[0])
            # Durable intent IDs bind the entire economic decision, not only
            # the broker account. Never reinterpret a saved order with new lot,
            # side, expiry, fees, risk permissions or Demo/REAL destination.
            # Mutable polling state and timing metadata are deliberately omitted.
            immutable=('login','server','currency','target','symbol','magic',
                       'kind','position','lots','signal','revision',
                       'deviation_points','created_ms','expires_ms',
                       'commission_per_lot','slippage_ticks','risk_percent',
                       'demo_flexible','micro_demo_risk_opt_in',
                       'micro_real_temporary','micro_real_cap_percent',
                       'micro_real_recovery_equity','daily_breaker_enabled',
                       'daily_breaker_limit_percent','max_quote_age_ms')
            if any(saved.get(k)!=cmd.get(k) for k in immutable):
                return dict(id=cmd['id'], state='unknown', reason='Intent identity differs from durable journal')
            return self._reconcile(mt5, saved, json.loads(row[1]) if row[1] else None)
        if cmd.get('state') in ('unknown', 'placed'):
            return dict(id=cmd['id'], state='unknown', reason='Missing gateway journal; inspect broker history')

        started = time.perf_counter()
        fast_path = is_fast_path(cmd)
        queue_ms = max(0, clock_ms()-cmd['created_ms'])
        result = dict(
            id=cmd['id'], state='rejected', reason='', fast_path=fast_path,
            queue_to_gateway_ms=queue_ms,
            fast_queue_target_ms=FAST_QUEUE_TARGET_MS,
            queue_target_met=queue_ms <= FAST_QUEUE_TARGET_MS,
        )
        if cmd.get('_core_pulse_us') is not None:
            result['core_pulse_us'] = cmd['_core_pulse_us']
            result['prefetched_work'] = True
        try:
            if clock_ms()>cmd['expires_ms']:
                raise ValueError('Intent expired before submission')
            a, terminal = mt5.account_info(), mt5.terminal_info()
            target_mode = 0 if cmd['target']=='demo' else 2 if cmd['target']=='live' else -1
            if account_identity(a)!=expected(cmd) or a.trade_mode!=target_mode:
                raise ValueError('Bound account or Demo/Live mode changed')
            if cmd.get('kind')=='open' and self._daily_loss_blocked(mt5,cmd,a):
                raise ValueError('Daily breaker: account daily loss limit reached or locked; new entries blocked')
            if (cmd.get('kind')=='open' and cmd.get('micro_real_temporary') is True
                    and not self._micro_real_active(cmd,a)):
                raise ValueError('Micro REAL recovery milestone reached/locked; stale relaxed-risk entry rejected')
            if not terminal or not terminal.connected or not terminal.trade_allowed or getattr(terminal,'tradeapi_disabled',False) or not a.trade_allowed or not a.trade_expert:
                raise ValueError('Enable Algo Trading and external Python trading in MT5; account must permit trading')
            if cmd['magic'] not in OWNED_MAGICS:
                raise ValueError('Unknown brain ownership')

            symbol=cmd['symbol']; info=mt5.symbol_info(symbol); tick=mt5.symbol_info_tick(symbol)
            if info is None or tick is None:
                raise ValueError('Broker contract/quote unavailable')
            if any(not math.isfinite(float(v)) or v<=0 for v in (info.point,info.trade_tick_size,info.volume_min,info.volume_step,info.volume_max,tick.bid,tick.ask)) or tick.ask<tick.bid:
                raise ValueError('Invalid broker contract/quote')
            age=clock_ms()-tick.time_msc
            if age < -2000 or age>cmd.get('max_quote_age_ms',3000):
                raise ValueError('Broker quote is stale')

            positions=mt5.positions_get(symbol=symbol); orders=mt5.orders_get(symbol=symbol)
            if positions is None or orders is None:
                raise ValueError('Position/order reconciliation unavailable')
            closing=cmd['kind']=='close'
            if closing:
                owned=[p for p in positions if p.ticket==cmd['position'] and p.magic==cmd['magic'] and p.symbol==symbol]
                if not owned:
                    return dict(result,state='closed',reason='Owned ticket is already absent; no order sent')
                p=owned[0]; lots=float(p.volume); side=-1 if p.type==mt5.POSITION_TYPE_BUY else 1
            else:
                if any(p.magic==cmd['magic'] for p in positions) or any(o.magic==cmd['magic'] for o in orders):
                    raise ValueError('Brain already owns a position/order')
                if a.margin_mode!=getattr(mt5,'ACCOUNT_MARGIN_MODE_RETAIL_HEDGING',2) and (positions or orders):
                    raise ValueError('Netting account: symbol occupied; independent ownership cannot be merged')
                lots=float(cmd['lots']);side=cmd['signal']['side']
                if side in (-1,1) and cmd.get('demo_flexible') is True and cmd['target']=='demo' and a.trade_mode==0:
                    sizing_started=time.perf_counter()
                    lots,result['sizing']=affordable_demo_volume(mt5,a,info,symbol,side,
                        float(tick.ask if side==1 else tick.bid),lots,float(cmd.get('commission_per_lot',0)))
                    result['sizing_ms']=(time.perf_counter()-sizing_started)*1000
            if side not in (-1,1) or not math.isfinite(lots) or not info.volume_min-1e-9<=lots<=info.volume_max+1e-9 or abs(lots/info.volume_step-round(lots/info.volume_step))>1e-7:
                raise ValueError('Volume/side does not match broker min/max/step')

            price=float(tick.ask if side==1 else tick.bid)
            fill_flags=getattr(info,'filling_mode',0)
            if fill_flags & 1:
                filling=mt5.ORDER_FILLING_FOK
            elif fill_flags & 2:
                filling=mt5.ORDER_FILLING_IOC
            elif getattr(info,'trade_exemode',None)!=getattr(mt5,'SYMBOL_TRADE_EXECUTION_MARKET',2):
                filling=mt5.ORDER_FILLING_RETURN
            else:
                raise ValueError('Broker provides no supported market filling policy')
            request=dict(action=mt5.TRADE_ACTION_DEAL,symbol=symbol,volume=lots,
                         type=mt5.ORDER_TYPE_BUY if side==1 else mt5.ORDER_TYPE_SELL,
                         price=price,deviation=int(cmd['deviation_points']),magic=cmd['magic'],
                         comment='BT:'+cmd['id'][:20],type_time=mt5.ORDER_TIME_GTC,type_filling=filling)
            if closing:
                request['position']=cmd['position']
            else:
                stop=float(cmd['signal']['stop_distance']);target=float(cmd['signal']['target_distance']);step=info.trade_tick_size
                if fast_path:
                    stop, target, minimum, spread = sniper_broker_geometry(info, tick, stop, target)
                    result['pricing'] = dict(
                        policy='sniper_send_geometry_v2_exact_tp',
                        quote_time_ms=int(tick.time_msc),
                        spread_points=spread/info.point,
                        broker_minimum_distance=minimum,
                        stop_distance=stop,
                        target_distance=target,
                    )
                if not all(math.isfinite(v) and v>0 for v in (stop,target)):
                    raise ValueError('Invalid stop/target geometry')
                request['sl']=(math.floor((price-side*stop)/step) if side==1 else math.ceil((price-side*stop)/step))*step
                request['tp']=(math.ceil((price+side*target)/step) if side==1 else math.floor((price+side*target)/step))*step
                exit_price=tick.bid if side==1 else tick.ask
                minimum=max(0,getattr(info,'trade_stops_level',0))*info.point
                if request['sl']<=0 or request['tp']<=0 or (exit_price-request['sl'])*side+step*1e-8<minimum or (request['tp']-exit_price)*side+step*1e-8<minimum:
                    raise ValueError('Stop/target too close for current spread or broker stops level')

                # This check runs for every Core-issued opening order, including
                # Sniper fast-path and REAL. Legacy REAL intents missing a risk
                # budget are rejected instead of bypassing the safety layer.
                if 'risk_percent' not in cmd:
                    if cmd['target']=='live':
                        raise ValueError('Risk sizing: REAL entry has no Core risk budget; rejected')
                else:
                    risk_started=time.perf_counter()
                    lots,result['risk_sizing']=size_to_stop_risk(
                        mt5,a,info,symbol,side,price,request['sl'],lots,
                        float(cmd['risk_percent']),
                        float(cmd.get('commission_per_lot',float('nan'))),
                        float(cmd.get('slippage_ticks',float('nan'))),
                        micro_demo_risk_opt_in=cmd.get('micro_demo_risk_opt_in') is True,
                        broker_demo=cmd['target']=='demo' and a.trade_mode==0,
                        micro_real_active=self._micro_real_active(cmd,a))
                    request['volume']=lots
                    result['risk_sizing_ms']=(time.perf_counter()-risk_started)*1000
                    if 'sizing' in result:
                        result['sizing']['post_risk_sent']=lots
                        result['sizing']['post_risk_round_trip_commission']=round_trip_commission(
                            float(cmd.get('commission_per_lot',0)),lots)

            # REAL margin is checked even on Sniper's no-order_check fast path.
            # No hidden leverage assumption: ask MT5 for this exact contract.
            if not closing and cmd['target']=='live':
                try:
                    margin=mt5.order_calc_margin(request['type'],symbol,request['volume'],price)
                except Exception as exc:
                    raise ValueError('REAL margin preflight: broker calculator failed') from exc
                free=float(getattr(a,'margin_free',float('nan')))
                if margin is None or not math.isfinite(float(margin)) or float(margin)<0 or not math.isfinite(free) or free<0:
                    raise ValueError('REAL margin preflight: broker margin or free margin unavailable')
                full_fee=round_trip_commission(float(cmd.get('commission_per_lot',float('nan'))),request['volume'])
                needed=float(margin)+full_fee
                if needed>free+1e-8:
                    raise ValueError(f'REAL margin preflight: needs {needed:.4f} {a.currency} incl. estimated fee, only {free:.4f} free')
                result['real_margin_check']=dict(estimated_margin=float(margin),free_margin=free,
                                                  full_roundtrip_fee=full_fee,required=needed)
            # Sniper removes one broker round trip. All equivalent local validations
            # above stay mandatory; the broker can still reject the final order_send.
            if fast_path:
                result['check_ms']=0.0
                result['order_check_skipped']=True
            else:
                check_start=time.perf_counter(); checked=mt5.order_check(request)
                result['check_ms']=(time.perf_counter()-check_start)*1000
                result['order_check_skipped']=False
                if checked is None or checked.retcode!=0:
                    raise ValueError('Broker order_check: '+str(getattr(checked,'comment','unavailable')))

            authorize_start=time.perf_counter()
            if not authorize(cmd['id']):
                result['authorize_ms']=(time.perf_counter()-authorize_start)*1000
                return dict(result,state='cancelled',reason='Core withdrew authorization before send')
            result['authorize_ms']=(time.perf_counter()-authorize_start)*1000

            current=mt5.account_info()
            if account_identity(current)!=expected(cmd) or current.trade_mode!=target_mode or clock_ms()>cmd['expires_ms']:
                raise ValueError('Account changed or intent expired during validation')

            journal_start=time.perf_counter()
            try:
                self.db.execute('INSERT INTO sends(id,request,result,created_ms) VALUES(?,?,NULL,?)',
                                (cmd['id'],json.dumps(dict(cmd,broker_request=request),allow_nan=False),clock_ms()))
                self.db.commit()
            except sqlite3.IntegrityError:
                # A concurrent worker already committed the same intent.
                # Reuse its durable state; never cross order_send twice.
                self.db.rollback()
                return self.execute(mt5,cmd,authorize)
            except sqlite3.OperationalError:
                # A locked/unwritable journal cannot guarantee idempotency.
                # Fail closed instead of dispatching without persistence.
                self.db.rollback()
                result.update(state='unknown',reason='Durable broker journal unavailable; order was not sent')
                result['total_ms']=(time.perf_counter()-started)*1000
                return result
            result['journal_ms']=(time.perf_counter()-journal_start)*1000
            result['request']=request

            # Closing the gap between the durable journal and order_send matters for
            # both STOP and account-switch semantics.  The first authorization avoids
            # journaling work that Core already withdrew; this second one is the true
            # last-moment gate after disk persistence.  If it fails, we know no broker
            # send occurred and persist an explicit terminal result instead of leaving
            # an ambiguous journal row.
            post_authorize_start=time.perf_counter()
            try:
                still_authorized=bool(authorize(cmd['id']))
            except Exception as exc:
                result['post_journal_authorize_ms']=(time.perf_counter()-post_authorize_start)*1000
                result.update(state='cancelled',reason='Final Core authorization unavailable after durable journal; order not sent: '+type(exc).__name__)
                result['total_ms']=(time.perf_counter()-started)*1000
                return self._save(cmd,result)
            result['post_journal_authorize_ms']=(time.perf_counter()-post_authorize_start)*1000
            if not still_authorized:
                result.update(state='cancelled',reason='Core withdrew authorization after durable journal; order not sent')
                result['total_ms']=(time.perf_counter()-started)*1000
                return self._save(cmd,result)

            post_account_start=time.perf_counter()
            final_account=mt5.account_info()
            result['post_journal_account_ms']=(time.perf_counter()-post_account_start)*1000
            if account_identity(final_account)!=expected(cmd) or getattr(final_account,'trade_mode',None)!=target_mode or clock_ms()>cmd['expires_ms']:
                result.update(state='cancelled',reason='Account changed or intent expired after durable journal; order not sent')
                result['total_ms']=(time.perf_counter()-started)*1000
                return self._save(cmd,result)
            if not closing and self._daily_loss_blocked(mt5,cmd,final_account):
                result.update(state='cancelled',
                    reason='Daily breaker reached/locked after validation; new entry not sent')
                result['total_ms']=(time.perf_counter()-started)*1000
                return self._save(cmd,result)
            if not closing and 'real_margin_check' in result:
                current_free=float(getattr(final_account,'margin_free',float('nan')))
                if (not math.isfinite(current_free)
                        or current_free+1e-8<result['real_margin_check']['required']):
                    result.update(state='cancelled',
                        reason='REAL free margin dropped below broker preflight requirement before send')
                    result['total_ms']=(time.perf_counter()-started)*1000
                    return self._save(cmd,result)
            if not closing and 'risk_sizing' in result:
                # Re-check the irrevocable REAL recovery milestone immediately
                # before send, even if it was crossed after initial sizing.
                if (cmd.get('micro_real_temporary') is True
                        and not self._micro_real_active(cmd,final_account)):
                    result.update(state='cancelled',
                        reason='Micro REAL recovery milestone reached or permanently locked; regular risk restored')
                    result['total_ms']=(time.perf_counter()-started)*1000
                    return self._save(cmd,result)
                # The broker equity may change while sizing and authorization run.
                # Never use a stale higher equity budget for the final send.
                try:
                    final_equity=float(getattr(final_account,'equity',float('nan')))
                except (TypeError,ValueError):
                    final_equity=float('nan')
                original_risk=result['risk_sizing']['estimated_stop_loss_including_costs']
                risk_percent=result['risk_sizing']['risk_percent']
                budget=final_equity*risk_percent/100
                if (not math.isfinite(budget) or budget<=0
                        or original_risk>budget+max(1e-8,budget*1e-10)):
                    result.update(state='cancelled',
                                  reason='Broker equity fell or became invalid after sizing; risk budget no longer permits this entry')
                    result['total_ms']=(time.perf_counter()-started)*1000
                    return self._save(cmd,result)

            result['pre_send_ms']=(time.perf_counter()-started)*1000
            result['queue_to_send_ms']=max(0,clock_ms()-cmd['created_ms'])
            result['queue_to_send_target_met']=result['queue_to_send_ms']<=FAST_QUEUE_TARGET_MS

            sent=time.perf_counter()
            try:
                response=mt5.order_send(request)
            except Exception as exc:
                result.update(state='unknown',reason='MT5 send interrupted: '+type(exc).__name__)
            else:
                code=getattr(response,'retcode',None)
                known_rejects={10004,10006,10007,10013,10014,10015,10016,10017,10018,10019,10020,10021,10022,10024,10026,10027,10028,10029,10030,10032,10033,10034,10035,10036,10038,10039,10040,10042,10043,10044,10045,10046}
                state={10009:'filled',10010:'partial',10008:'placed'}.get(code,'rejected' if code in known_rejects else 'unknown')
                order_id=int(getattr(response,'order',0) or 0)
                deal_id=int(getattr(response,'deal',0) or 0)
                # A DONE/DONE_PARTIAL retcode without any deal ticket is not
                # sufficient evidence of an exact fill/volume/price. Preserve
                # the send journal and reconcile using subsequent MT5 history
                # instead of training on an acknowledgement with deal=0.
                if state in ('filled','partial') and deal_id<=0:
                    state='placed'
                result.update(state=state,retcode=code,reason=str(getattr(response,'comment','No broker acknowledgement')),
                              order=order_id,deal=deal_id,
                              volume=float(getattr(response,'volume',0)),price=float(getattr(response,'price',0)))
                if deal_id<=0 and code in (10009,10010):
                    result['reason']='Broker retcode acknowledges submission but deal ticket is unavailable; await exact MT5 history reconciliation'
            result['acknowledged_ms']=clock_ms()
            result['send_ms']=(time.perf_counter()-sent)*1000
            result['total_ms']=(time.perf_counter()-started)*1000
            return self._save(cmd,result)
        except (ValueError,TypeError,AttributeError) as exc:
            result['reason']=str(exc)
            result['total_ms']=(time.perf_counter()-started)*1000
            return result

    def cycle(self,mt5,api,work=None):
        """Execute one Core work item, optionally reusing work prefetched by a pulse."""
        work=api('/v1/execution/gateway') if work is None else work
        command=work.get('command')
        if command:
            if work.get('pulse_to_work_us') is not None:
                command=dict(command)
                command['_core_pulse_us']=work['pulse_to_work_us']
            result=self.execute(mt5,command,lambda intent:api('/v1/execution/authorize',{'id':intent}).get('authorized',False))
            api('/v1/execution/result',result)
        # Pulse work deliberately defers history synchronization to a later normal
        # cycle so broker history cannot delay the next latency-sensitive quote.
        if work.get('pulse_fast_path'):
            return work.get('poll_ms',100)/1000
        start=work.get('history_from_ms',0);identity=work.get('account')
        if start and identity and time.monotonic()>=self.next_history:
            self.next_history=time.monotonic()+5
            wanted=(identity['login'],identity['server'],identity['currency'])
            until=min(clock_ms(),start+86_400_000)
            if account_identity(mt5.account_info())==wanted:
                rows=mt5.history_deals_get(datetime.fromtimestamp(start/1000,timezone.utc),datetime.fromtimestamp(until/1000,timezone.utc))
                if rows is not None and account_identity(mt5.account_info())==wanted:
                    fields=('ticket','position_id','magic','symbol','entry','type','time_msc','volume','price','profit','commission','swap','fee','reason')
                    deals=[{k:getattr(d,k,0) for k in fields} for d in rows]
                    api('/v1/execution/sync',dict(identity,deals=deals,until_ms=until))
        return work.get('poll_ms',250)/1000
