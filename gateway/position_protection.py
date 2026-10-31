"""Durable MT5 SL modification for a single, previously identified position.

This transport does not create protection policies or initiate work by itself.
Core must issue and authorize each immutable intent. In particular, a broker
acknowledgement without an observed position is not a successful modification.
"""
import json
import math
import sqlite3
import time


def _now():
    return int(time.time() * 1000)


def _identity(account):
    return (getattr(account, 'login', None), getattr(account, 'server', None),
            getattr(account, 'currency', None)) if account else None


def _account_ok(mt5, cmd):
    account = mt5.account_info()
    mode = {'demo': 0, 'live': 2}.get(cmd.get('target'), -1)
    return (_identity(account) == (cmd.get('login'), cmd.get('server'), cmd.get('currency'))
            and getattr(account, 'trade_mode', None) == mode)


def _number(value, name, minimum=0.0):
    if isinstance(value, bool):
        raise ValueError('Invalid ' + name)
    number = float(value)
    if not math.isfinite(number) or number < minimum:
        raise ValueError('Invalid ' + name)
    return number


def _position(mt5, cmd):
    positions = mt5.positions_get(symbol=cmd['symbol'])
    if positions is None:
        raise ValueError('MT5 positions unavailable')
    matches = [p for p in positions if getattr(p, 'ticket', None) == cmd['position']]
    if len(matches) != 1:
        raise ValueError('Exact position unavailable')
    p = matches[0]
    side = 'BUY' if getattr(p, 'type', None) == mt5.POSITION_TYPE_BUY else (
        'SELL' if getattr(p, 'type', None) == getattr(mt5, 'POSITION_TYPE_SELL', 1) else '')
    if (getattr(p, 'symbol', None) != cmd['symbol'] or getattr(p, 'magic', None) != cmd['magic']
            or getattr(p, 'identifier', None) != cmd['expected_position_identifier']
            or side != cmd['expected_position_side']
            or abs(_number(getattr(p, 'volume', None), 'position volume', 1e-12)
                   - _number(cmd['expected_position_volume'], 'approved volume', 1e-12)) > 1e-8):
        raise ValueError('Position identity, side or volume changed')
    return p, side


def _observed(executor, mt5, cmd, result):
    """Resolve by broker-visible position state; never call order_send again."""
    if result.get('state') not in ('unknown', 'placed'):
        return result
    try:
        if not _account_ok(mt5, cmd):
            return result
        p, side = _position(mt5, cmd)
        info = mt5.symbol_info(cmd['symbol'])
        step = _number(getattr(info, 'trade_tick_size', None), 'tick size', 1e-12)
        sl = _number(getattr(p, 'sl', None), 'position SL')
        tp = _number(getattr(p, 'tp', None), 'position TP')
        desired = _number(cmd['protect_stop'], 'requested SL', 1e-12)
        expected_tp = _number(cmd['expected_position_tp'], 'approved TP')
        favourable = sl > 0 and (sl >= desired - step*1e-7 if side == 'BUY'
                                 else sl <= desired + step*1e-7)
        if favourable and abs(tp - expected_tp) <= step*1e-7 and _account_ok(mt5, cmd):
            result = dict(result, state='filled', reason='Requested or tighter SL observed on the exact MT5 position',
                          protection_observed=dict(position=cmd['position'], identifier=cmd['expected_position_identifier'],
                                                   sl=sl, tp=tp, observed_ms=_now()))
            return executor._save(cmd, result)
    except (AttributeError, KeyError, TypeError, ValueError, OverflowError, OSError, RuntimeError):
        pass
    return result


def _validate(mt5, cmd, owned_magics, brain_magic):
    if (cmd.get('kind') != 'protect' or cmd.get('magic') not in owned_magics
            or brain_magic.get(cmd.get('brain')) != cmd.get('magic')):
        raise ValueError('Invalid protection owner')
    if (cmd.get('target') not in ('demo','live') or not isinstance(cmd.get('id'), str) or not cmd['id']
            or not isinstance(cmd.get('protect_policy_id'), str) or not cmd['protect_policy_id']
            or type(cmd.get('position')) is not int or cmd['position'] <= 0
            or type(cmd.get('expected_position_identifier')) is not int or cmd['expected_position_identifier'] <= 0
            or type(cmd.get('created_ms')) is not int or type(cmd.get('expires_ms')) is not int
            or type(cmd.get('max_quote_age_ms')) is not int or not 0 < cmd['max_quote_age_ms'] <= 3000
            or cmd['expires_ms'] < cmd['created_ms'] or _now() > cmd['expires_ms']):
        raise ValueError('Invalid or expired protection intent')
    if not _account_ok(mt5, cmd):
        raise ValueError('Bound account or Demo/Live mode changed')
    terminal, account = mt5.terminal_info(), mt5.account_info()
    if (_identity(account) != (cmd['login'], cmd['server'], cmd['currency'])
            or getattr(account, 'trade_mode', None) != {'demo': 0, 'live': 2}.get(cmd['target'], -1)
            or not terminal or not getattr(terminal, 'connected', False)
            or not getattr(terminal, 'trade_allowed', False) or getattr(terminal, 'tradeapi_disabled', False)
            or not getattr(account, 'trade_allowed', False) or not getattr(account, 'trade_expert', False)
            or not _account_ok(mt5, cmd)):
        raise ValueError('MT5 terminal or account does not permit trading')
    info, tick = mt5.symbol_info(cmd['symbol']), mt5.symbol_info_tick(cmd['symbol'])
    point = _number(getattr(info, 'point', None), 'point', 1e-12)
    step = _number(getattr(info, 'trade_tick_size', None), 'tick size', 1e-12)
    bid, ask = _number(getattr(tick, 'bid', None), 'bid', 1e-12), _number(getattr(tick, 'ask', None), 'ask', 1e-12)
    tick_ms = getattr(tick, 'time_msc', None)
    if (ask < bid or type(tick_ms) is not int or _now() - tick_ms < -2000
            or _now() - tick_ms > cmd.get('max_quote_age_ms', 3000)):
        raise ValueError('Broker quote is invalid or stale')
    p, side = _position(mt5, cmd)
    old_sl = _number(getattr(p, 'sl', None), 'position SL')
    old_tp = _number(getattr(p, 'tp', None), 'position TP')
    old_open = _number(getattr(p, 'price_open', None), 'position entry', 1e-12)
    desired = _number(cmd['protect_stop'], 'requested SL', 1e-12)
    expected_sl = _number(cmd['expected_position_sl'], 'approved SL')
    expected_tp = _number(cmd['expected_position_tp'], 'approved TP')
    expected_open = _number(cmd['expected_position_open'], 'approved entry', 1e-12)
    eps = step * 1e-7
    if (abs(old_sl-expected_sl)>eps or abs(old_tp-expected_tp)>eps or abs(old_open-expected_open)>eps):
        raise ValueError('Position SL, TP or entry changed')
    if abs(desired / step - round(desired / step)) > 1e-6:
        raise ValueError('Requested SL is outside the broker tick grid')
    if old_sl > 0 and (desired <= old_sl+eps if side == 'BUY' else desired >= old_sl-eps):
        raise ValueError('Protection must tighten, never loosen, the existing SL')
    exit_quote = bid if side == 'BUY' else ask
    distance = exit_quote-desired if side == 'BUY' else desired-exit_quote
    stops = _number(getattr(info, 'trade_stops_level', None), 'broker stops level') * point
    freeze = _number(getattr(info, 'trade_freeze_level', None), 'broker freeze level') * point
    if distance <= eps or distance+eps < max(stops, freeze):
        raise ValueError('Requested SL is inside the broker stops/freeze distance')
    if old_sl > 0 and abs(exit_quote-old_sl)+eps < freeze:
        raise ValueError('Existing SL is within the broker freeze distance')
    if old_tp > 0 and abs(exit_quote-old_tp)+eps < freeze:
        raise ValueError('Existing TP is within the broker freeze distance')
    return dict(action=mt5.TRADE_ACTION_SLTP, symbol=cmd['symbol'], position=cmd['position'], sl=desired, tp=old_tp)


def execute_protection(executor, mt5, cmd, authorize, owned_magics, brain_magic, max_auth_age_ms):
    immutable = ('login','server','currency','target','symbol','magic','brain','kind','position',
                 'expected_position_identifier','expected_position_side','expected_position_volume',
                 'expected_position_open','expected_position_sl','expected_position_tp','protect_stop',
                 'protect_policy_id','created_ms','expires_ms','max_quote_age_ms')
    row = executor.db.execute('SELECT request,result FROM sends WHERE id=?', (cmd['id'],)).fetchone()
    if row:
        saved = json.loads(row[0])
        if any(saved.get(k) != cmd.get(k) for k in immutable):
            return dict(id=cmd['id'], state='unknown', reason='Protection intent differs from its durable journal')
        result = json.loads(row[1]) if row[1] else dict(id=cmd['id'], state='unknown', reason='Interrupted; no automatic retry')
        return _observed(executor, mt5, saved, result)
    if cmd.get('state') not in ('queued','dispatched'):
        return dict(id=cmd['id'], state='unknown', reason='Old protection intent without a Gateway journal; never resend')
    result = dict(id=cmd['id'], state='rejected', reason='', fast_path=False)
    journaled = False
    sent = False
    try:
        prior = executor.db.execute('''SELECT 1 FROM sends WHERE id<>? AND json_valid(request)
            AND json_extract(request,'$.kind')='protect' AND json_extract(request,'$.login')=?
            AND json_extract(request,'$.server')=? AND json_extract(request,'$.currency')=?
            AND json_extract(request,'$.target')=? AND json_extract(request,'$.position')=?
            AND (result IS NULL OR CASE WHEN json_valid(result)
                 THEN COALESCE(json_extract(result,'$.state') NOT IN ('filled','cancelled','rejected'),1)
                 ELSE 1 END)
            LIMIT 1''', (cmd['id'],cmd['login'],cmd['server'],cmd['currency'],cmd['target'],cmd['position'])).fetchone()
        if prior:
            return dict(result, state='cancelled', reason='Earlier modification for this position remains unresolved')
        request = _validate(mt5, cmd, owned_magics, brain_magic)
        checked = mt5.order_check(request)
        if checked is None or getattr(checked, 'retcode', None) != 0:
            raise ValueError('Broker order_check rejected SL modification: '+str(getattr(checked,'comment','unavailable')))
        if not authorize(cmd['id']):
            return dict(result, state='cancelled', reason='Core withdrew protection authorization')
        try:
            executor.db.execute('INSERT INTO sends(id,request,result,created_ms) VALUES(?,?,NULL,?)',
                                (cmd['id'], json.dumps(dict(cmd, broker_request=request), allow_nan=False), _now()))
            executor.db.commit()
            journaled = True
        except sqlite3.IntegrityError:
            executor.db.rollback()
            return execute_protection(executor, mt5, cmd, authorize, owned_magics, brain_magic, max_auth_age_ms)
        except sqlite3.Error:
            executor.db.rollback()
            return dict(result, state='unknown', reason='Durable journal unavailable; modification was not sent')
        result['request'] = request
        if not authorize(cmd['id']):
            return executor._save(cmd, dict(result, state='cancelled', reason='Core withdrew authorization after journal; not sent'))
        authorized_at = time.perf_counter()
        final_request = _validate(mt5, cmd, owned_magics, brain_magic)
        if (final_request != request or not _account_ok(mt5, cmd) or _now() > cmd['expires_ms']
                or (time.perf_counter()-authorized_at)*1000 > max_auth_age_ms):
            return executor._save(cmd, dict(result, state='cancelled', reason='Final position, price or authorization changed; not sent'))
        sent = True
        try:
            response = mt5.order_send(request)
            code = getattr(response, 'retcode', None)
            known_rejects = {10004,10006,10007,10013,10014,10015,10016,10017,10018,10019,10020,10021,10022,10024,10026,10027,10029,10030,10032,10033,10034,10035,10036,10038,10040,10042,10043,10044,10045,10046}
            state = 'placed' if code in (10008,10009) else ('rejected' if code in known_rejects else 'unknown')
            result.update(state=state, retcode=code, reason=str(getattr(response,'comment','No broker acknowledgement')),
                          acknowledged_ms=_now())
        except Exception as exc:
            result.update(state='unknown', reason='MT5 modification interrupted: '+type(exc).__name__)
        executor._save(cmd, result)
        return _observed(executor, mt5, cmd, result)
    except (ValueError,TypeError,KeyError,AttributeError,OverflowError,OSError,RuntimeError,sqlite3.Error) as exc:
        result.update(state='unknown' if sent else ('cancelled' if journaled else 'rejected'), reason=str(exc))
        if journaled:
            try:
                return executor._save(cmd, result)
            except sqlite3.Error:
                pass
        return result
