"""One-shot MT5 pending-order cancellation with exact terminal reconciliation.

Only a Core-authorized immutable ticket may be sent. Absence from active orders
does not prove cancellation: a fill, expiry or partial execution may have won
the race. The exact MT5 terminal order history remains the source of outcome.
"""
import json
import math
import sqlite3
import time
from position_protection import _account_ok, _now, _number


def _order_identity(order, cmd, *, active):
    numeric=(('volume_initial','expected_volume_initial'),('price_open','expected_price_open'),
             ('sl','expected_sl'),('tp','expected_tp'),('price_stoplimit','expected_stoplimit'))
    if (getattr(order,'ticket',None)!=cmd['order'] or getattr(order,'symbol',None)!=cmd['symbol']
            or getattr(order,'magic',None)!=cmd['magic'] or getattr(order,'type',None)!=cmd['expected_order_type']
            or getattr(order,'time_setup_msc',None)!=cmd['expected_setup_ms']
            or getattr(order,'time_expiration',None)!=cmd['expected_expiration']):
        return False
    if any(abs(_number(getattr(order,field,None),field)-_number(cmd[key],key))>1e-8
           for field,key in numeric):
        return False
    initial=_number(getattr(order,'volume_initial',None),'initial volume',1e-12)
    current=_number(getattr(order,'volume_current',None),'remaining volume')
    return current<=initial+1e-8 and (not active or abs(current-_number(cmd['expected_volume_current'],'approved remainder',1e-12))<=1e-8)


def _terminal(mt5,cmd):
    if not _account_ok(mt5,cmd) or not callable(getattr(mt5,'history_orders_get',None)):
        return None
    active=mt5.orders_get(ticket=cmd['order'])
    history=mt5.history_orders_get(ticket=cmd['order'])
    if active is None or len(active)!=0 or history is None or len(history)!=1 or not _account_ok(mt5,cmd):
        return None
    item=history[0]
    states={getattr(mt5,name,None):state for name,state in (
        ('ORDER_STATE_CANCELED','cancelled'),('ORDER_STATE_FILLED','filled'),
        ('ORDER_STATE_EXPIRED','expired'),('ORDER_STATE_REJECTED','rejected')) if getattr(mt5,name,None) is not None}
    state=states.get(getattr(item,'state',None))
    if state is None or not _order_identity(item,cmd,active=False):return None
    return dict(confirmed=True,ticket=cmd['order'],state=state,
                volume_initial=float(item.volume_initial),volume_current=float(item.volume_current))


def _reconcile(executor,mt5,cmd,result,has_journal):
    if result['state'] not in ('unknown','placed'):return result
    try:
        evidence=_terminal(mt5,cmd)
        if evidence:
            result=dict(result,state=evidence['state'],terminal_order=evidence,
                        reason='Exact broker order history observed; fills may already exist')
            return executor._save(cmd,result) if has_journal else result
    except (AttributeError,KeyError,TypeError,ValueError,OverflowError,OSError,RuntimeError,sqlite3.Error):pass
    return result


def _validate(mt5,cmd,owned_magics,brain_magic):
    if (cmd.get('target') not in ('demo','live') or cmd.get('kind')!='cancel_pending'
            or cmd.get('magic') not in owned_magics or brain_magic.get(cmd.get('brain'))!=cmd.get('magic')
            or type(cmd.get('order')) is not int or cmd['order']<=0
            or type(cmd.get('expected_order_type')) is not int or cmd['expected_order_type'] not in range(2,8)
            or type(cmd.get('expected_setup_ms')) is not int or cmd['expected_setup_ms']<0
            or type(cmd.get('expected_expiration')) is not int or cmd['expected_expiration']<0
            or type(cmd.get('expires_ms')) is not int or _now()>cmd['expires_ms']):
        raise ValueError('Invalid or expired pending-order cancellation')
    if not _account_ok(mt5,cmd):raise ValueError('Bound account or Demo/Live mode changed')
    terminal,account=mt5.terminal_info(),mt5.account_info()
    if (not terminal or not getattr(terminal,'connected',False) or not getattr(terminal,'trade_allowed',False)
            or getattr(terminal,'tradeapi_disabled',False) or not getattr(account,'trade_allowed',False)
            or not getattr(account,'trade_expert',False) or not _account_ok(mt5,cmd)):
        raise ValueError('MT5 terminal or account does not permit cancellation')
    rows=mt5.orders_get(ticket=cmd['order'])
    if rows is None:raise ValueError('MT5 active order list unavailable')
    if len(rows)!=1 or not _order_identity(rows[0],cmd,active=True):
        raise ValueError('Exact order changed, filled or is no longer active')
    if not _account_ok(mt5,cmd):raise ValueError('Account changed during order validation')
    return dict(action=mt5.TRADE_ACTION_REMOVE,order=cmd['order'])


def execute_cancellation(executor,mt5,cmd,authorize,owned_magics,brain_magic,max_auth_age_ms):
    immutable=('login','server','currency','target','symbol','magic','brain','kind','order',
               'expected_order_type','expected_volume_initial','expected_volume_current','expected_price_open',
               'expected_sl','expected_tp','expected_stoplimit','expected_setup_ms','expected_expiration','created_ms','expires_ms')
    row=executor.db.execute('SELECT request,result FROM sends WHERE id=?',(cmd['id'],)).fetchone()
    if row:
        saved=json.loads(row[0])
        if any(saved.get(k)!=cmd.get(k) for k in immutable):
            return dict(id=cmd['id'],state='unknown',reason='Cancellation identity differs from durable journal')
        result=json.loads(row[1]) if row[1] else dict(id=cmd['id'],state='unknown',reason='Interrupted; no automatic retry')
        return _reconcile(executor,mt5,saved,result,True)
    if cmd.get('state') not in ('queued','dispatched'):
        return _reconcile(executor,mt5,cmd,dict(id=cmd['id'],state='unknown',reason='No journal; never resend'),False)
    result=dict(id=cmd['id'],state='rejected',reason='',fast_path=False);journaled=False;sent=False
    try:
        prior=executor.db.execute('''SELECT 1 FROM sends WHERE id<>? AND json_valid(request)
            AND json_extract(request,'$.kind')='cancel_pending' AND json_extract(request,'$.login')=?
            AND json_extract(request,'$.server')=? AND json_extract(request,'$.currency')=?
            AND json_extract(request,'$.target')=? AND json_extract(request,'$.order')=?
            AND (result IS NULL OR CASE WHEN json_valid(result)
              THEN COALESCE(json_extract(result,'$.state') NOT IN ('cancelled','filled','expired','rejected'),1)
              ELSE 1 END) LIMIT 1''',(cmd['id'],cmd['login'],cmd['server'],cmd['currency'],cmd['target'],cmd['order'])).fetchone()
        if prior:return dict(result,state='unknown',reason='An earlier cancellation for this order is unresolved')
        evidence=_terminal(mt5,cmd)
        if evidence:return dict(result,state=evidence['state'],terminal_order=evidence,reason='Order already terminal; no cancellation sent')
        request=_validate(mt5,cmd,owned_magics,brain_magic);check=mt5.order_check(request)
        if check is None or getattr(check,'retcode',None)!=0:
            raise ValueError('Broker order_check rejected cancellation: '+str(getattr(check,'comment','unavailable')))
        if not authorize(cmd['id']):return dict(result,state='cancelled',reason='Core withdrew authorization; not sent')
        try:
            executor.db.execute('INSERT INTO sends(id,request,result,created_ms) VALUES(?,?,NULL,?)',
                                (cmd['id'],json.dumps(dict(cmd,broker_request=request),allow_nan=False),_now()))
            executor.db.commit();journaled=True
        except sqlite3.IntegrityError:
            executor.db.rollback();return execute_cancellation(executor,mt5,cmd,authorize,owned_magics,brain_magic,max_auth_age_ms)
        except sqlite3.Error:
            executor.db.rollback();return dict(result,state='unknown',reason='Durable journal unavailable; cancellation not sent')
        result['request']=request
        if not authorize(cmd['id']):return executor._save(cmd,dict(result,state='cancelled',reason='Core withdrew authorization after journal; not sent'))
        authorized_at=time.perf_counter();final=_validate(mt5,cmd,owned_magics,brain_magic)
        if (final!=request or not _account_ok(mt5,cmd) or _now()>cmd['expires_ms']
                or (time.perf_counter()-authorized_at)*1000>max_auth_age_ms):
            return executor._save(cmd,dict(result,state='cancelled',reason='Final account, order or authorization changed; not sent'))
        sent=True
        try:
            response=mt5.order_send(request);code=getattr(response,'retcode',None)
            known_rejects={10004,10006,10007,10013,10014,10015,10016,10017,10018,10019,10020,10021,10022,10024,10026,10027,10029,10030,10032,10033,10034,10035,10036,10038,10040,10042,10043,10044,10045,10046}
            result.update(state='placed' if code in (10008,10009) else ('rejected' if code in known_rejects else 'unknown'),
                          retcode=code,reason=str(getattr(response,'comment','No broker acknowledgement')))
        except Exception as exc:result.update(state='unknown',reason='MT5 cancellation interrupted: '+type(exc).__name__)
        executor._save(cmd,result);return _reconcile(executor,mt5,cmd,result,True)
    except (ValueError,TypeError,KeyError,AttributeError,OverflowError,OSError,RuntimeError,sqlite3.Error) as exc:
        result.update(state='unknown' if sent else ('cancelled' if journaled else 'rejected'),reason=str(exc))
        if journaled:
            try:return executor._save(cmd,result)
            except sqlite3.Error:pass
        return result
