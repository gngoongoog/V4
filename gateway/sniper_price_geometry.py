"""Price-distance geometry for Sniper orders.

Distances are expressed in the instrument's quoted price (e.g. 0.25 in XAUUSD),
not pips, points, or account-currency P/L. This module is deliberately pure:
it does not send orders or modify positions.
"""
import math


def _finite_positive(value, name):
    if isinstance(value, bool):
        raise ValueError(f"{name} must be a positive finite price distance")
    value = float(value)
    if not math.isfinite(value) or value <= 0:
        raise ValueError(f"{name} must be a positive finite price distance")
    return value


def sniper_levels_from_fill(info, side, fill_price, stop_distance, target_distance):
    """Return exact broker-grid SL/TP levels measured from the confirmed fill.

    side: +1 for BUY, -1 for SELL.
    Refuses unrepresentable distances rather than silently widening them.
    """
    if side not in (-1, 1):
        raise ValueError("Sniper side must be BUY (+1) or SELL (-1)")
    fill = _finite_positive(fill_price, "Actual fill price")
    stop = _finite_positive(stop_distance, "Stop distance")
    target = _finite_positive(target_distance, "Target distance")
    point = _finite_positive(getattr(info, "point", None), "Broker point")
    step = _finite_positive(getattr(info, "trade_tick_size", None), "Broker tick size")
    stops_level = getattr(info, "trade_stops_level", 0)
    freeze_level = getattr(info, "trade_freeze_level", 0)
    if isinstance(stops_level, bool) or isinstance(freeze_level, bool):
        raise ValueError("Invalid broker stops/freeze level")
    stops_level, freeze_level = int(stops_level), int(freeze_level)
    if stops_level < 0 or freeze_level < 0:
        raise ValueError("Invalid broker stops/freeze level")

    # A price distance must map to an exact integer number of tradable ticks.
    for name, distance in (("Stop", stop), ("Target", target)):
        ticks = distance / step
        if abs(ticks - round(ticks)) > 1e-7:
            raise ValueError(
                f"{name} distance {distance:.8f} cannot be represented exactly "
                f"with broker tick size {step:.8f}; value was not rounded or widened"
            )
    fill_ticks = fill / step
    if abs(fill_ticks - round(fill_ticks)) > 1e-7:
        raise ValueError("Confirmed fill price is outside the broker tick grid")

    sl = (round(fill_ticks) - side * round(stop / step)) * step
    tp = (round(fill_ticks) + side * round(target / step)) * step
    if sl <= 0 or tp <= 0:
        raise ValueError("Calculated Sniper SL/TP is not a valid positive price")
    min_distance = max(stops_level, freeze_level) * point
    if stop + 1e-9 < min_distance or target + 1e-9 < min_distance:
        raise ValueError(
            f"Requested distance is below broker stops/freeze minimum "
            f"{min_distance:.8f}; SL/TP was not widened"
        )
    return {
        "sl": sl,
        "tp": tp,
        "fill_price": fill,
        "stop_distance": stop,
        "target_distance": target,
        "tick_size": step,
        "point": point,
        "broker_minimum_distance": min_distance,
        "policy": "sniper_price_distance_from_confirmed_fill_v1",
    }
