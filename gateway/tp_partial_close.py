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


def stage_volume(cmd, account, mt5, position, info):
    """Validate exact broker-owned, broker-grid planned close volume.

    Returns (close_lots, closing_side). No order_send or rounding occurs.
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
