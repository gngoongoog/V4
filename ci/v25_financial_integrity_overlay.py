"""Apply P0-D financial truth and account-mode gates to reconstructed V24.

Fail closed on an unexpected restored source revision. No live MT5 connection
or order is made by this build step.
"""
from pathlib import Path

def checked_replace(text,old,new,file):
    if text.count(new)==1:
        return text
    if text.count(old)!=1:
        raise SystemExit(f"Unexpected {file} P0-D source anchor; refusing unverified build")
    return text.replace(old,new,1)

snapshot=Path("gateway/mt5_gateway.py")
s=snapshot.read_text(encoding="utf-8")
s=checked_replace(s,
    '''    trading = [d for d in deals if d.type in (mt5.DEAL_TYPE_BUY, mt5.DEAL_TYPE_SELL)]
    net = sum(d.profit + d.commission + d.swap + getattr(d, "fee", 0.0) for d in trading)
    return net, len(trading)
''',
    '''    trading = [d for d in deals if d.type in (mt5.DEAL_TYPE_BUY, mt5.DEAL_TYPE_SELL)]
    # A missing broker-reported cost is unknown, never equivalent to a
    # zero-commission account. Likewise, NaN/Inf must not reach Core/UI as PnL.
    booked = []
    for deal in trading:
        components = []
        for key in ('profit', 'commission', 'swap', 'fee'):
            value = getattr(deal, key, None)
            if isinstance(value, bool):
                return None, None
            try:
                amount = float(value)
            except (ValueError, TypeError, OverflowError):
                return None, None
            if not math.isfinite(amount):
                return None, None
            components.append(amount)
        booked.append(math.fsum(components))
    net = math.fsum(booked)
    if not math.isfinite(net):
        return None, None
    return net, len(trading)
''',"MT5 daily booked net")
s=checked_replace(s,
    '''    if current is None or (current.login, current.server) != (account.login, account.server):
        return empty_snapshot("Account changed during snapshot; waiting for reconciliation.")
''',
    '''    identity_fields=('login','server','currency','trade_mode')
    if (current is None or
        tuple(getattr(current,k,None) for k in identity_fields) !=
        tuple(getattr(account,k,None) for k in identity_fields)):
        return empty_snapshot("Account changed during snapshot; waiting for reconciliation.")
''',"MT5 account-mode snapshot identity")
s=checked_replace(s,
    '''    if deals is None:
        errors.append("Deal history unavailable; daily PnL is unknown.")
''',
    '''    if deals is None:
        errors.append("Deal history unavailable; daily PnL is unknown.")
    elif net is None:
        errors.append("Broker deal cost fields missing or invalid; daily PnL is unknown.")
''',"MT5 invalid cost audit warning")
snapshot.write_text(s,encoding="utf-8")

journal=Path("gateway/execution.py")
s=journal.read_text(encoding="utf-8")
s=checked_replace(s,
    '''            value=sum(float(getattr(d,k,0.0)) for k in ('profit','commission','swap','fee'))
            if not math.isfinite(value):
                raise ValueError('Daily breaker: invalid broker deal costs')
            net+=value
''',
    '''            # No guessed zero commission / swap / fee in REAL risk decisions.
            # Unknown cost data blocks NEW entries but never position closure.
            components=[]
            for k in ('profit','commission','swap','fee'):
                raw=getattr(d,k,None)
                if isinstance(raw,bool):
                    raise ValueError('Daily breaker: invalid broker deal costs')
                try:
                    component=float(raw)
                except (ValueError,TypeError,OverflowError) as exc:
                    raise ValueError('Daily breaker: invalid broker deal costs') from exc
                if not math.isfinite(component):
                    raise ValueError('Daily breaker: invalid broker deal costs')
                components.append(component)
            net+=math.fsum(components)
            if not math.isfinite(net):
                raise ValueError('Daily breaker: invalid broker deal costs')
''',"MT5 daily risk ledger")
journal.write_text(s,encoding="utf-8")
print("V25 P0-D financial truth guards staged (no MT5 order sent)")
