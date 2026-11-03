"""Fail-closed DEMO/hedging partial-TP close contract.

This is a validation helper used by the existing durable Gateway close path.
It does NOT create a Core broker command, stage price, completion event or
authorization. Until a Core-owned stage executor exists, it cannot initiate
a TP close on its own.
"""
from __future__ import annotations

import math
import re

_PLAN = re.compile(r"tp-plan-[0-9a-f]{32}\Z")


def _number(value, name):
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"TP stage {name} must be a finite number")
    result = float(value)
    if not math.isfinite(result):
        raise ValueError(f"TP stage {name} must be finite")
    return result


def stage_volume(cmd, account, mt5, position, info, tick=None, now_ms=None):
    """Validate exact partial-close size AND a fresh executable TP price.

    Both the early Gateway preflight and the final physical-position re-read
    must pass a broker Bid/Ask quote. No rounding, order_send or retry here.
    """
    plan = cmd.get("tp_stage_plan_id")
    if not isinstance(plan, str) or not _PLAN.fullmatch(plan):
        raise ValueError("TP stage has no exact immutable Core plan identity")
    stage = cmd.get("tp_stage")
    if type(stage) is not int or stage not in (1, 2, 3):
        raise ValueError("TP stage number must be 1, 2, or 3")
    if cmd.get("kind") != "close" or cmd.get("target") != "demo":
        raise ValueError("TP stage is DEMO close only; LIVE stage sends are disabled")
    if account is None or getattr(account, "trade_mode", None) != 0:
        raise ValueError("TP stage requires the bound MT5 DEMO account")
    if getattr(account, "margin_mode", None) != getattr(mt5, "ACCOUNT_MARGIN_MODE_RETAIL_HEDGING", 2):
        raise ValueError("TP stage requires hedging mode; netting is not implemented")
    if cmd.get("manual_close") is True or cmd.get("manual_open") is True:
        raise ValueError("TP stage cannot borrow manual close/open permissions")
    if type(cmd.get("expected_position_identifier")) is not int:
        raise ValueError("TP stage requires a positive physical position identifier")
    identifier = cmd["expected_position_identifier"]
    if identifier <= 0 or identifier != getattr(position, "identifier", None):
        raise ValueError("TP stage physical position identifier changed")
    if getattr(position, "ticket", None) != cmd.get("position") or getattr(position, "magic", None) != cmd.get("magic"):
        raise ValueError("TP stage ticket or strategy ownership changed")
    side_type = getattr(position, "type", None)
    if side_type not in (getattr(mt5, "POSITION_TYPE_BUY", 0), getattr(mt5, "POSITION_TYPE_SELL", 1)):
        raise ValueError("TP stage has invalid physical position side")
    name = "BUY" if side_type == getattr(mt5, "POSITION_TYPE_BUY", 0) else "SELL"
    if cmd.get("expected_position_side") != name:
        raise ValueError("TP stage position side changed")
    # A TP stage is a price-conditioned close, never a blind market close.
    # The immutable Core command must name its intended broker-tick-aligned
    # price. The quote must be independently obtained from MT5 on every check.
    target_price = _number(cmd.get("tp_target_price"), "target price")
    price_step = _number(getattr(info, "trade_tick_size", None), "broker tick size")
    open_price = _number(getattr(position, "price_open", None), "position entry")
    bound_entry = _number(cmd.get("expected_position_entry"), "original position entry")
    current_broker_tp = _number(getattr(position, "tp", None), "original MT5 TP")
    bound_broker_tp = _number(cmd.get("expected_broker_tp"), "original bound MT5 TP")
    if min(target_price, price_step, open_price, bound_entry) <= 0 or min(current_broker_tp,bound_broker_tp)<0:
        raise ValueError("TP stage invalid target, tick size, original entry or broker TP")
    # MT5 can retain the same position identifier and ticket through some
    # account/broker operations; immutable economic geometry must also match
    # the original Core-saved price proposal at every pre-send check.
    tolerance = price_step * 1e-6
    if not math.isclose(open_price, bound_entry, rel_tol=0, abs_tol=tolerance):
        raise ValueError("TP stage original position entry changed")
    if not math.isclose(current_broker_tp, bound_broker_tp, rel_tol=0, abs_tol=tolerance):
        raise ValueError("TP stage original MT5 full-position TP changed")
    # The original broker TP must not flatten the whole position before a
    # staged target. For zero TP, staged prices are the only saved proposal.
    if bound_broker_tp:
        if (name=="BUY" and bound_broker_tp<=target_price+price_step*0.5) or (
                name=="SELL" and bound_broker_tp>=target_price-price_step*0.5):
            raise ValueError("TP stage original broker full TP conflicts with partial target")
    price_units = target_price / price_step
    if not math.isfinite(price_units) or abs(price_units-round(price_units)) > 1e-6:
        raise ValueError("TP stage target not on broker price tick grid")
    if name == "BUY" and target_price <= open_price+price_step*0.5:
        raise ValueError("TP stage BUY price must be above position entry")
    if name == "SELL" and target_price+price_step*0.5 >= open_price:
        raise ValueError("TP stage SELL price must be below position entry")
    if tick is None or type(now_ms) is not int or now_ms<=0:
        raise ValueError("TP stage requires a fresh MT5 source quote")
    bid = _number(getattr(tick, "bid", None), "Bid")
    ask = _number(getattr(tick, "ask", None), "Ask")
    stamp = getattr(tick, "time_msc", None)
    max_age = cmd.get("max_quote_age_ms", 3000)
    if (min(bid,ask)<=0 or ask<bid or type(stamp) is not int or stamp<=0
            or type(max_age) is not int or not 1<=max_age<=3000
            or stamp>now_ms+2000 or now_ms-stamp>max_age):
        raise ValueError("TP stage quote is invalid, crossed or stale")
    exit_price = bid if name == "BUY" else ask
    if (name == "BUY" and exit_price+price_step*1e-7<target_price) or (
            name == "SELL" and exit_price>target_price+price_step*1e-7):
        raise ValueError("TP stage broker executable price has not reached target")
    held = _number(getattr(position, "volume", None), "position volume")
    original = _number(cmd.get("expected_position_volume"), "expected position volume")
    lots = _number(cmd.get("lots"), "requested partial lots")
    minimum = _number(getattr(info, "volume_min", None), "broker minimum")
    maximum = _number(getattr(info, "volume_max", None), "broker maximum")
    step = _number(getattr(info, "volume_step", None), "broker volume step")
    if min(held, original, lots, minimum, maximum, step) <= 0 or minimum > maximum:
        raise ValueError("TP stage has invalid broker volume rules")
    if not math.isclose(held, original, rel_tol=1e-9, abs_tol=step * 1e-6):
        raise ValueError("TP stage position volume changed from Core-approved snapshot")
    for value, key in ((held, "position"), (lots, "close")):
        units = value / step
        if not math.isfinite(units) or abs(units - round(units)) > 1e-6:
            raise ValueError(f"TP stage {key} volume not on broker step")
    if not minimum - step * 1e-7 <= lots <= min(held, maximum) + step * 1e-7:
        raise ValueError("TP stage close volume outside exact broker limits")
    remaining = held - lots
    if stage in (1, 2):
        if remaining < minimum - step * 1e-7 or abs(remaining / step - round(remaining / step)) > 1e-6:
            raise ValueError("TP stage would leave an untradeable residual")
    elif abs(remaining) > step * 1e-6:
        raise ValueError("TP3 must close the whole remaining physical position")
    return lots, (-1 if name == "BUY" else 1)
