"""Read-only High Activity Node (HVN) estimator from *observed MT5 ticks*.

No OHLC candle-range spreading and NO invented market-wide volume.
This module is a research primitive, not a broker signal or trading strategy.
It accepts MT5 copy_ticks_range records, including numpy structured rows.
When MT5 reports actual last-price trade volume_real, it uses that alone.
Otherwise it counts broker quote changes at the observed bid/ask midpoint,
clearly marking those nodes as quote-activity proxies, NOT traded HVNs.
"""
from __future__ import annotations

from collections import defaultdict
from math import floor, isfinite
from numbers import Integral
from statistics import median

MAX_TICKS = 40_000
MAX_LEVELS = 2_000
MAX_NODES = 8
MAX_WINDOW_MS = 3_600_000
MIN_SPAN_MS = 30_000


def _field(row, name, default=None):
    if isinstance(row, dict):
        return row.get(name, default)
    try:
        return row[name]
    except (TypeError, KeyError, ValueError, IndexError):
        return getattr(row, name, default)


def _number(value):
    try:
        result = float(value)
    except (ValueError, TypeError, OverflowError):
        return None
    return result if isfinite(result) else None


def _unavailable(reason, identity, window_ms):
    return {
        "available": False, "read_only": True, "trade_authorized": False,
        "identity": identity, "window_ms": window_ms,
        "source": "none", "unit": "none", "broker_only": True,
        "exchange_wide": False, "nodes": [], "reason": reason,
    }


def calculate_hvn(ticks, *, server, login, symbol, currency, tick_size,
                  now_ms, window_ms=600_000, min_events=80):
    """A causal, account-scoped profile using only past observed ticks.

    Every node has a price level and a *measured* broker tick-event count or
    broker-reported real traded volume; its intensity is descriptive only.
    Results are NEVER probabilities of winning, hidden liquidity or orders.
    """
    identity = {"server": server, "login": login,
                "symbol": symbol, "currency": currency}
    if not server or not symbol or not currency or not isinstance(login, int) or login <= 0:
        return _unavailable("MT5 account identity missing", identity, window_ms)
    step = _number(tick_size)
    if step is None or step <= 0 or step > 1000:
        return _unavailable("Broker tick_size unavailable or invalid", identity, window_ms)
    if (not isinstance(now_ms, int) or now_ms <= 0
            or not isinstance(window_ms, int) or not 60_000 <= window_ms <= MAX_WINDOW_MS
            or not isinstance(min_events, int) or not 50 <= min_events <= 1000):
        return _unavailable("Invalid causal window or evidence threshold", identity, window_ms)
    if ticks is None or not hasattr(ticks, "__len__") or len(ticks) > MAX_TICKS:
        return _unavailable("Tick history missing or larger than the bounded audit window", identity, window_ms)

    trades, quotes = [], []
    previous_time = 0
    for row in ticks:
        timestamp = _number(_field(row, "time_msc"))
        if timestamp is None or timestamp != int(timestamp):
            return _unavailable("Tick timestamp missing or malformed", identity, window_ms)
        timestamp = int(timestamp)
        if timestamp < previous_time:
            return _unavailable("Out-of-order MT5 tick history was rejected", identity, window_ms)
        previous_time = timestamp
        if timestamp < now_ms - window_ms or timestamp > now_ms + 2000:
            continue
        if timestamp > now_ms:
            # Do not allow a broker clock ahead of the decision to leak future data.
            continue

        raw_flags = _field(row, "flags")
        # Last/volume_real can be carried forward on a quote-only tick. Missing,
        # coerced or unknown flags cannot prove a *new* broker trade/quote;
        # reject the entire report rather than invent measured transactions.
        if (isinstance(raw_flags, bool) or not isinstance(raw_flags, Integral)
                or int(raw_flags) < 0 or int(raw_flags) & ~(2 | 4 | 8 | 16 | 32 | 64)):
            return _unavailable("Missing or invalid MT5 tick change flags; provenance unverified",
                                identity, window_ms)
        flags = int(raw_flags)
        last = _number(_field(row, "last"))
        real_volume = _number(_field(row, "volume_real"))
        trade_change = bool(flags & (8 | 16 | 32 | 64))
        quote_change = bool(flags & (2 | 4))
        if (trade_change and last is not None and last > 0
                and real_volume is not None and real_volume > 0):
            trades.append((timestamp, last, real_volume))
        elif quote_change:
            bid = _number(_field(row, "bid"))
            ask = _number(_field(row, "ask"))
            if bid is not None and ask is not None and bid > 0 and ask >= bid:
                # ONE changed quote, not a transaction. Never infer buyer/seller.
                quotes.append((timestamp, (bid + ask) / 2.0, 1.0))

    if len(trades) >= min_events:
        samples = trades
        source, unit = "mt5_broker_trade_volume_real", "broker_reported_real_volume"
        note = "Broker-reported transaction last-price volume, not consolidated exchange-wide volume"
    else:
        samples = quotes
        source, unit = "mt5_quote_tick_activity", "observed_quote_events"
        note = "Broker quote-event activity at bid/ask midpoint; NOT traded volume or order flow"
    if len(samples) < min_events:
        return _unavailable("Insufficient observed broker ticks; no HVN is inferred", identity, window_ms)
    if samples[-1][0] - samples[0][0] < MIN_SPAN_MS:
        return _unavailable("Observed ticks cover less than 30 seconds", identity, window_ms)

    weights = defaultdict(float)
    for _, price, weight in samples:
        level = floor(price / step + 0.5)
        weights[level] += weight
        if len(weights) > MAX_LEVELS:
            return _unavailable("Too many distinct price levels for bounded HVN audit", identity, window_ms)
    if len(weights) < 5:
        return _unavailable("Too few distinct price levels to identify a meaningful node", identity, window_ms)
    total = sum(weights.values())
    if not isfinite(total) or total <= 0:
        return _unavailable("Nonfinite or empty broker activity", identity, window_ms)
    active_mean = total / len(weights)
    typical = median(weights.values())
    raw_nodes = []
    for level, activity in weights.items():
        left, right = weights.get(level - 1, 0.0), weights.get(level + 1, 0.0)
        # A local peak must dominate its immediately adjacent price levels
        # and the average activity of nonempty levels.
        if (activity >= active_mean * 1.4
                and activity >= typical * 1.5
                and activity >= max(left, right) * 1.2
                and activity / total >= 0.02):
            raw_nodes.append({"price": round(level * step, 10),
                              "activity": round(activity, 6),
                              "share": round(activity / total, 6)})
    raw_nodes.sort(key=lambda v: (-v["activity"], v["price"]))
    if not raw_nodes:
        return _unavailable("No statistically descriptive local peak in observed ticks", identity, window_ms)
    return {
        "available": True, "read_only": True, "trade_authorized": False,
        "identity": identity, "window_ms": window_ms,
        "observed_from_ms": samples[0][0], "observed_to_ms": samples[-1][0],
        "observations": len(samples), "distinct_levels": len(weights),
        "source": source, "unit": unit, "broker_only": True,
        "exchange_wide": False, "nodes": raw_nodes[:MAX_NODES],
        "total_activity": round(total, 6),
        "note": note,
        "reason": "Read-only research evidence; not validated for MT5 order execution",
    }

def collect_mt5_hvn(mt5, *, server, login, symbol, currency,
                    now_ms, window_ms=600_000):
    """One explicitly requested broker-scoped read; never in the Sniper hot path.

    Caller owns the MT5 terminal and must ensure trading is unaffected.
    This function does not initialize, change symbols, send orders or shut MT5.
    """
    from datetime import datetime, timezone, timedelta

    identity = {"server": server, "login": login,
                "symbol": symbol, "currency": currency}
    if not isinstance(now_ms, int) or now_ms <= 0 or not isinstance(window_ms, int) or not 60_000 <= window_ms <= MAX_WINDOW_MS:
        return _unavailable("Invalid bounded MT5 tick research interval", identity, window_ms)

    def same_account():
        account = mt5.account_info()
        return (account is not None
                and getattr(account, "login", None) == login
                and getattr(account, "server", None) == server
                and getattr(account, "currency", None) == currency)

    def flat_account():
        # Live MT5 read, independent of the earlier Core snapshot. None from
        # the broker is UNKNOWN, never equivalent to zero open positions.
        positions = mt5.positions_get()
        orders = mt5.orders_get()
        return (positions is not None and orders is not None
                and len(positions) == 0 and len(orders) == 0)

    try:
        if not same_account():
            return _unavailable("MT5 account changed or identity mismatched", identity, window_ms)
        if not flat_account():
            return _unavailable("MT5 has open positions, orders, or an unknown order book; historical tick read withheld", identity, window_ms)
        info = mt5.symbol_info(symbol)
        if info is None:
            return _unavailable("Broker symbol not found; check exact symbol suffix", identity, window_ms)
        end = datetime.fromtimestamp(now_ms / 1000, timezone.utc)
        start = end - timedelta(milliseconds=window_ms)
        ticks = mt5.copy_ticks_range(symbol, start, end, mt5.COPY_TICKS_ALL)
        if not same_account():
            return _unavailable("MT5 account changed during tick history read", identity, window_ms)
        if not flat_account():
            return _unavailable("MT5 became nonflat or broker order state unknown during tick history read", identity, window_ms)
        return calculate_hvn(ticks, server=server, login=login, symbol=symbol,
                             currency=currency,
                             tick_size=getattr(info, "trade_tick_size", None),
                             now_ms=now_ms, window_ms=window_ms)
    except (AttributeError, OSError, RuntimeError, ValueError, TypeError, OverflowError) as error:
        return _unavailable("MT5 tick history read failed: " + str(error)[:140],
                            identity, window_ms)
