"""Broker-booked trading PnL and standalone commission, not cash transfers."""
import math


def commission_types(mt5):
    # ENUM_DEAL_TYPE values are part of the MT5 API; named constants are
    # preferred when provided by a particular Python package version.
    return {getattr(mt5, name, value) for name, value in (
        ('DEAL_TYPE_COMMISSION', 7), ('DEAL_TYPE_COMMISSION_DAILY', 8),
        ('DEAL_TYPE_COMMISSION_MONTHLY', 9), ('DEAL_TYPE_COMMISSION_AGENT_DAILY', 10),
        ('DEAL_TYPE_COMMISSION_AGENT_MONTHLY', 11))}


def booked_trading_net(deals, mt5):
    """Return account-wide booked net and trade count; unknown costs fail closed.

    Standalone commission is included in the account total only. It is not
    attributed to an individual position without independent broker evidence.
    Balance/credit transfers remain excluded.
    """
    trading = {mt5.DEAL_TYPE_BUY, mt5.DEAL_TYPE_SELL}
    fees = commission_types(mt5)
    amounts, count = [], 0
    for deal in deals:
        kind = getattr(deal, 'type', None)
        if kind not in trading and kind not in fees:
            continue
        if kind in trading:
            count += 1
        parts = []
        for key in ('profit', 'commission', 'swap', 'fee'):
            raw = getattr(deal, key, None)
            if isinstance(raw, bool):
                raise ValueError('Invalid broker deal costs')
            try:
                value = float(raw)
            except (ValueError, TypeError, OverflowError) as exc:
                raise ValueError('Invalid broker deal costs') from exc
            if not math.isfinite(value):
                raise ValueError('Invalid broker deal costs')
            parts.append(value)
        try:
            amounts.append(math.fsum(parts))
        except OverflowError as exc:
            raise ValueError('Invalid broker deal costs') from exc
    try:
        net = math.fsum(amounts)
    except OverflowError as exc:
        raise ValueError('Invalid broker deal costs') from exc
    if not math.isfinite(net):
        raise ValueError('Invalid broker deal costs')
    return net, count
