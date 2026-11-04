"""Offline MT5 link resilience tests; no real terminal or order_send calls."""
from __future__ import annotations
from datetime import datetime,timedelta,timezone
from pathlib import Path
from types import SimpleNamespace as N
from unittest import TestCase,main
from unittest.mock import patch
import sys

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/"gateway"))
from mt5_gateway import (
    DailyDealsCache,CoreOutageBudget,MT5DisconnectBudget,
    CoreFallbackThrottle,collect,mt5_session_alive,
    SNIPER_FULL_SNAPSHOT_SECONDS,NATIVE_BATCH_MAX
)

class FakeBroker:
    ACCOUNT_TRADE_MODE_DEMO=0
    POSITION_TYPE_BUY=0
    DEAL_TYPE_BUY=0
    DEAL_TYPE_SELL=1
    def __init__(self):
        self.account=N(login=17,server="Broker-Demo",currency="USD",trade_mode=0,
                       balance=100.,equity=100.,margin=0.,margin_free=100.,
                       trade_allowed=True,trade_expert=True,leverage=500)
        self.terminal=N(connected=True,trade_allowed=True,tradeapi_disabled=False)
        self.positions=[]
        self.orders=[]
        self.deals=[N(type=0,profit=5.,commission=-.7,swap=0.,fee=0.)]
        self.history_calls=0
        self.positions_calls=0
        self.orders_calls=0
        self.raise_history=False
        self.shutdown_calls=0
    def terminal_info(self):return self.terminal
    def account_info(self):return self.account
    def positions_get(self):
        self.positions_calls+=1
        return self.positions
    def orders_get(self):
        self.orders_calls+=1
        return self.orders
    def history_deals_get(self,start,end):
        self.history_calls+=1
        if self.raise_history:raise RuntimeError("synthetic MT5 IPC hiccup")
        return self.deals
    def symbol_info(self,symbol):
        return N(visible=True,volume_min=.01,volume_step=.01,
                 trade_tick_size=.01,volume_max=100.,point=.01)
    def symbol_info_tick(self,symbol):
        return N(bid=2000.,ask=2000.2,time_msc=1000)
    def shutdown(self):self.shutdown_calls+=1

class MT5ConnectionResilienceTests(TestCase):
    def test_reconciled_snapshots_do_not_refetch_daily_deals_each_half_second(self):
        b=FakeBroker();cache=DailyDealsCache(interval=5.)
        now=datetime(2026,11,4,6,15,tzinfo=timezone.utc)
        with patch("mt5_gateway.time.monotonic",side_effect=[1.,1.3,1.9,2.7,4.9,6.1]):
            rows=[collect(b,"XAUUSD",now,deals_cache=cache) for _ in range(6)]
        self.assertEqual(b.history_calls,2)
        self.assertEqual(b.positions_calls,6)
        self.assertEqual(b.orders_calls,6)
        self.assertTrue(all(row["connected"] for row in rows))
        self.assertTrue(all(row["today_net"]==4.3 for row in rows))
        self.assertTrue(all(row["today_deals"]==1 for row in rows))
        self.assertGreaterEqual(SNIPER_FULL_SNAPSHOT_SECONDS,.5)
        self.assertLessEqual(NATIVE_BATCH_MAX,32)

    def test_daily_history_separates_login_server_live_mode_currency_and_baghdad_date(self):
        b=FakeBroker();cache=DailyDealsCache()
        now=datetime(2026,11,3,20,59,59,tzinfo=timezone.utc)
        with patch("mt5_gateway.time.monotonic",return_value=1.):
            self.assertEqual(cache.read(b,b.account,now)[0],4.3)
            cache.read(b,b.account,now)
            self.assertEqual(b.history_calls,1)
            b.account.login=18;cache.read(b,b.account,now)
            b.account.server="Other";cache.read(b,b.account,now)
            b.account.trade_mode=2;cache.read(b,b.account,now)
            b.account.currency="EUR";cache.read(b,b.account,now)
            self.assertEqual(b.history_calls,5)
            cache.read(b,b.account,now+timedelta(seconds=2))
            self.assertEqual(b.history_calls,6)

    def test_history_failure_never_fabricates_zero_or_borrows_other_account_costs(self):
        b=FakeBroker();cache=DailyDealsCache()
        now=datetime(2026,11,4,6,15,tzinfo=timezone.utc)
        with patch("mt5_gateway.time.monotonic",side_effect=[1.,7.,8.,9.]):
            self.assertEqual(collect(b,"XAUUSD",now,deals_cache=cache)["today_net"],4.3)
            b.raise_history=True
            row=collect(b,"XAUUSD",now,deals_cache=cache)
            self.assertTrue(row["connected"])
            self.assertIsNone(row["today_net"])
            self.assertIsNone(row["today_deals"])
            self.assertIn("Deal history unavailable",row["error"])
            b.account.trade_mode=2
            self.assertIsNone(collect(b,"XAUUSD",now,deals_cache=cache)["today_net"])
            b.raise_history=False;b.account.login=12345
            self.assertEqual(collect(b,"XAUUSD",now,deals_cache=cache)["today_net"],4.3)

    def test_unverified_portfolio_is_closed_to_trading_but_one_ipc_hiccup_does_not_logout(self):
        b=FakeBroker();b.positions=None
        result=collect(b,"XAUUSD")
        self.assertFalse(result["connected"])
        self.assertEqual(result["positions"],[])
        self.assertIn("trading paused",result["error"])
        self.assertTrue(mt5_session_alive(b))
        self.assertEqual(b.shutdown_calls,0)
        b.positions=[]
        self.assertTrue(collect(b,"XAUUSD")["connected"])
        b.terminal.connected=False
        self.assertFalse(mt5_session_alive(b))
        b.terminal.connected=True;b.account=None
        self.assertFalse(mt5_session_alive(b))

    def test_three_core_http_misses_in_a_few_milliseconds_do_not_restart_gateway(self):
        budget=CoreOutageBudget(grace=20.)
        for when in (100.,100.01,100.02):
            self.assertFalse(budget.failure(when))
        self.assertFalse(budget.expired(119.9))
        self.assertTrue(budget.expired(120.))
        budget.success()
        self.assertFalse(budget.expired(120.1))
        self.assertFalse(budget.failure(130.))
        self.assertFalse(budget.failure(131.))
        self.assertFalse(budget.failure(132.))
        self.assertTrue(budget.failure(151.))

    def test_one_terminal_ipc_error_does_not_trigger_shutdown_or_resume_stale_orders(self):
        budget=MT5DisconnectBudget(threshold=3,grace=1.)
        self.assertFalse(budget.offline(10.))
        self.assertFalse(budget.offline(10.5))
        budget.healthy()
        self.assertFalse(budget.offline(11.))
        self.assertFalse(budget.offline(11.4))
        self.assertTrue(budget.offline(12.))
        budget.healthy()
        self.assertEqual(budget.count,0)

    def test_broken_pulse_fallback_cannot_hammer_core_at_tick_frequency(self):
        limiter=CoreFallbackThrottle(interval=.5)
        self.assertTrue(limiter.allow(10.))
        self.assertFalse(limiter.allow(10.005))
        self.assertFalse(limiter.allow(10.2))
        self.assertTrue(limiter.allow(10.5))
        self.assertFalse(limiter.allow(10.5001))
        self.assertTrue(limiter.allow(11.))

if __name__=="__main__":
    main()
