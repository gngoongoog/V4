"""Read-only MT5 DEMO scenario preview using broker contract math.

No order_send, order_check, broker mutation, config changes, training, or
permission to trade. Stop and TP are hypothetical, not promised fill prices.
"""
from __future__ import annotations
import argparse
import json
import math
from pathlib import Path
import sys
import time

from demo_evidence_export import DemoEvidenceError, identity, finite_cash, positive

class PreviewError(DemoEvidenceError):
    pass

def val(obj, attr):
    return getattr(obj, attr, None) if obj is not None else None

def finite(v):
    return isinstance(v, (int,float)) and not isinstance(v,bool) and math.isfinite(v)

def valid_positive(v):
    return finite(v) and 0 < v <= 1e12

def calculate(mt5, symbol, side, lots, stop_price, take_profit=None,
              roundtrip_commission_per_lot=7.0, slippage_ticks_per_side=1.0,
              risk_percent=1.0, max_quote_age_ms=3000, now_ms=None):
    symbol = identity(symbol, "Symbol")
    if side not in ("buy", "sell"):
        raise PreviewError("Side must be buy or sell")
    if not all(valid_positive(v) for v in (lots, stop_price)):
        raise PreviewError("Positive lots and stop price are required")
    if take_profit is not None and not valid_positive(take_profit):
        raise PreviewError("Take-profit price is invalid")
    if not finite(roundtrip_commission_per_lot) or not 0 <= roundtrip_commission_per_lot <= 1000:
        raise PreviewError("Invalid full roundtrip commission per lot")
    if not finite(slippage_ticks_per_side) or not 0 <= slippage_ticks_per_side <= 100:
        raise PreviewError("Invalid per-side slippage assumption")
    if not finite(risk_percent) or not 0 < risk_percent <= 20:
        raise PreviewError("Preview-only risk percent must be within 0–20")
    if type(max_quote_age_ms) is not int or not 1 <= max_quote_age_ms <= 3000:
        raise PreviewError("Invalid quote age limit")
    now_ms = int(time.time()*1000) if now_ms is None else now_ms
    if type(now_ms) is not int or now_ms <= 0:
        raise PreviewError("Invalid capture time")
    terminal, account = mt5.terminal_info(), mt5.account_info()
    demo_mode = getattr(mt5,"ACCOUNT_TRADE_MODE_DEMO",None)
    if terminal is None or not val(terminal,"connected") or account is None or demo_mode is None or val(account,"trade_mode") != demo_mode:
        raise PreviewError("Only a connected MT5 Demo account is supported")
    login=identity(val(account,"login"),"Login")
    server=identity(val(account,"server"),"Server")
    currency=identity(val(account,"currency"),"Currency")
    key=(login,server,currency,demo_mode)
    equity=val(account,"equity")
    free_margin=val(account,"margin_free")
    if not valid_positive(equity) or not finite(free_margin) or free_margin < 0:
        raise PreviewError("Fresh positive equity and finite nonnegative free margin required")
    info,tick=mt5.symbol_info(symbol),mt5.symbol_info_tick(symbol)
    if info is None or tick is None or val(info,"visible") is not True:
        raise PreviewError("Visible MT5 symbol and quote required; symbol selection is not modified")
    bid,ask = val(tick,"bid"),val(tick,"ask")
    quote_time = val(tick,"time_msc")
    if not valid_positive(bid) or not valid_positive(ask) or ask < bid or type(quote_time) is not int or quote_time<=0 or quote_time>now_ms or now_ms-quote_time>max_quote_age_ms:
        raise PreviewError("Missing, future or stale executable-side Demo quote")
    minimum,maximum,step,unit=tuple(val(info,k) for k in (
        "volume_min","volume_max","volume_step","trade_tick_size"))
    if not all(valid_positive(v) for v in (minimum,maximum,step,unit)) or maximum<minimum:
        raise PreviewError("Missing broker lot/tick contract")
    steps=(lots-minimum)/step
    if lots < minimum-1e-8 or lots > maximum+1e-8 or abs(steps-round(steps)) > 1e-7:
        raise PreviewError("Requested lot does not satisfy Demo broker min/max/step; no rounding up")
    if side=="buy":
        if not stop_price < bid:
            raise PreviewError("BUY stop must lie strictly below the Demo bid")
        if take_profit is not None and not take_profit>ask:
            raise PreviewError("BUY target must lie strictly above the Demo ask")
        order_type=getattr(mt5,"ORDER_TYPE_BUY",None)
        assumed_open=ask+slippage_ticks_per_side*unit
        assumed_stop=stop_price-slippage_ticks_per_side*unit
        assumed_tp=(take_profit-slippage_ticks_per_side*unit if take_profit is not None else None)
    else:
        if not stop_price > ask:
            raise PreviewError("SELL stop must lie strictly above the Demo ask")
        if take_profit is not None and not take_profit<bid:
            raise PreviewError("SELL target must lie strictly below the Demo bid")
        order_type=getattr(mt5,"ORDER_TYPE_SELL",None)
        assumed_open=bid-slippage_ticks_per_side*unit
        assumed_stop=stop_price+slippage_ticks_per_side*unit
        assumed_tp=(take_profit+slippage_ticks_per_side*unit if take_profit is not None else None)
    if order_type is None or not all(valid_positive(x) for x in (assumed_open,assumed_stop)):
        raise PreviewError("Unsupported MT5 order side or adverse quote scenario")
    commission=roundtrip_commission_per_lot*lots
    try:
        broker_stop=mt5.order_calc_profit(order_type,symbol,lots,assumed_open,assumed_stop)
        broker_margin=mt5.order_calc_margin(order_type,symbol,lots,assumed_open)
        broker_target=(mt5.order_calc_profit(order_type,symbol,lots,assumed_open,assumed_tp)
                       if assumed_tp is not None else None)
    except Exception as e:
        raise PreviewError("MT5 Demo broker calculation raised an error") from e
    if (not finite(broker_stop) or broker_stop>=0
            or not finite(broker_margin) or broker_margin<0
            or (assumed_tp is not None and not finite(broker_target))):
        raise PreviewError("Demo broker stop/margin/target monetary calculation unavailable or inconsistent")
    budget=equity*risk_percent/100.
    loss=-broker_stop+commission
    margin_with_fee=broker_margin+commission
    if not all(finite(v) and abs(v)<=1e12 for v in (commission,budget,loss,margin_with_fee)):
        raise PreviewError("Broker risk preview exceeds numerical bounds")
    final_terminal,final_account=mt5.terminal_info(),mt5.account_info()
    after=(str(val(final_account,"login")),str(val(final_account,"server")),
           str(val(final_account,"currency")),val(final_account,"trade_mode"))
    if final_terminal is None or not val(final_terminal,"connected") or after != key:
        raise PreviewError("Demo account changed or disconnected during broker calculation")
    return {
        "policy":"demo_broker_contract_math_preview_v1",
        "status":"read_only_broker_calculation_estimates_not_authorization",
        "account":login,"server":server,"currency":currency,
        "demo":True,"symbol":symbol,"side":side,
        "quote":{"bid":bid,"ask":ask,"quote_age_ms":now_ms-quote_time,
                 "spread_ticks":(ask-bid)/unit,"tick_size":unit},
        "requested_lots":lots,"broker_volume_min":minimum,
        "broker_volume_max":maximum,"broker_volume_step":step,
        "assumed_entry_with_slippage":assumed_open,
        "assumed_stop_fill_with_slippage":assumed_stop,
        "assumed_target_fill_with_slippage":assumed_tp,
        "configured_commission_roundtrip_per_lot":roundtrip_commission_per_lot,
        "estimated_roundtrip_commission":commission,
        "estimated_broker_stop_gross":broker_stop,
        "estimated_loss_if_stop_including_assumed_costs":loss,
        "estimated_broker_target_gross":broker_target,
        "estimated_target_net_after_assumed_commission":
            broker_target-commission if broker_target is not None else None,
        "estimated_initial_margin":broker_margin,
        "estimated_margin_plus_assumed_commission":margin_with_fee,
        "demo_equity":equity,"demo_free_margin":free_margin,
        "scenario_risk_percent":risk_percent,"scenario_risk_budget":budget,
        "scenario_stop_risk_exceeds_budget":loss>budget,
        "scenario_margin_plus_fees_exceeds_free_margin":margin_with_fee>free_margin,
        "source":"MT5 Demo order_calc_profit and order_calc_margin, NOT order_check",
        "broker_order_check_called":False,"broker_order_sent":False,
        "complete_portfolio_or_margin_admission":False,
        "actual_broker_approval":False,"profitability_verified":False,
        "warning":"Only a standalone Demo read-only estimate. Real fees may differ, spreads can widen, stop loss may gap, margin may change with current positions/hedging, and no actual order acceptance or profitability is certified."
    }

def main(argv=None):
    p=argparse.ArgumentParser(description="Only DEMO read-only MT5 monetary profit/margin preview")
    p.add_argument("--symbol",required=True)
    p.add_argument("--side",choices=("buy","sell"),required=True)
    p.add_argument("--lots",type=float,required=True)
    p.add_argument("--stop",type=float,required=True)
    p.add_argument("--target",type=float)
    p.add_argument("--roundtrip-commission-per-lot",type=float,default=7.)
    p.add_argument("--slippage-ticks-per-side",type=float,default=1.)
    p.add_argument("--risk-percent",type=float,default=1.)
    p.add_argument("--output",help="Create a new research report instead of printing")
    args=p.parse_args(argv)
    try:
        try:
            import MetaTrader5 as mt5
        except ImportError as exc:
            raise PreviewError("MetaTrader5 Python package unavailable") from exc
        if not mt5.initialize():
            raise PreviewError("Could not connect to the current local MT5 terminal")
        try:
            result=calculate(mt5,args.symbol,args.side,args.lots,args.stop,
                take_profit=args.target,roundtrip_commission_per_lot=args.roundtrip_commission_per_lot,
                slippage_ticks_per_side=args.slippage_ticks_per_side,risk_percent=args.risk_percent)
        finally:
            mt5.shutdown()
        output=json.dumps(result,indent=2,ensure_ascii=False,allow_nan=False)
        if args.output:
            with Path(args.output).open("x",encoding="utf-8") as f:
                f.write(output+"\n")
        else:
            print(output)
        return 0
    except (PreviewError,OSError) as e:
        print("Read-only Demo margin/risk preview refused: "+str(e),file=sys.stderr)
        return 2

if __name__=="__main__":
    raise SystemExit(main())
