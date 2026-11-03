"""Offline account-selected TP partial-close Gateway regressions.

The fixture deliberately supplies a synthetic Core-approved command; the
production Core does NOT yet generate these intents. No real broker is called.
"""
from __future__ import annotations

import sys
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace as N

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/"gateway"))
from execution import BrokerExecutor, is_fast_path, clock_ms
from test_broker_execution import Broker, command
from tp_partial_close import stage_volume

PLAN="tp-plan-"+"a"*32


def stage_command(stage=1,held=0.1,lots=0.05):
    c=command()
    c.update(id="tp-req-"+str(stage)+"-"+"b"*24,kind="close",position=91,
        brain="gann",magic=410601,lots=lots,signal=None,
        tp_stage_plan_id="tp-plan-"+{1:"a",2:"b",3:"c"}.get(stage,"a")*32,
        tp_stage=stage,
        tp_target_price=round(1999.9+(stage-1)*0.01,2),
        expected_position_entry=1999.5,expected_broker_tp=0.0,
        expected_position_identifier=917,
        expected_position_volume=held,expected_position_side="BUY")
    return c


class StageDemoTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(prefix="bt-stage-demo-")
        self.ex=BrokerExecutor(Path(self.tmp.name)/"gateway.sqlite")
        self.mt=Broker()
        self.p=N(ticket=91,identifier=917,magic=410601,symbol="XAUUSD",
            volume=0.1,type=0,profit=8.,price_open=1999.5,tp=0.0)
        self.mt.positions=[self.p]
        self.cmd=stage_command()
    def tearDown(self):
        self.ex.close();self.tmp.cleanup()
    def send(self,authorize=lambda _:True):
        return self.ex.execute(self.mt,self.cmd,authorize)
    def history_fill(self, lots=None, order=55, position=917, entry=1, side=1,
                     magic=None, deal=77):
        """Only immutable synthetic history can complete a TP stage."""
        self.mt.deals=[N(ticket=deal,order=order,position_id=position,
            magic=self.cmd["magic"] if magic is None else magic,
            symbol=self.cmd["symbol"],
            volume=self.cmd["lots"] if lots is None else lots,
            type=side,entry=entry,comment="BT:"+self.cmd["id"][:20],
            price=2000.0,time_msc=clock_ms())]
    def test_tp1_exact_lots_preflight_order_check_and_journal_replay(self):
        self.assertFalse(is_fast_path(self.cmd))
        result=self.send()
        self.assertEqual(result["state"],"placed",result)
        self.assertEqual(result["retcode"],10009)
        self.assertFalse(result.get("recovered_from_history",False))
        self.assertEqual(result["request"]["volume"],0.05)
        self.assertEqual(self.mt.sends[0]["position"],91)
        self.assertFalse(result.get("order_check_skipped"))
        self.assertEqual(len(self.mt.sends),1)
        self.assertEqual(self.send()["state"],"placed",
                         "A bare DONE acknowledgement cannot prove a TP fill")
        self.history_fill()
        verified=self.send()
        self.assertEqual(verified["state"],"filled",verified)
        self.assertTrue(verified["recovered_from_history"])
        self.assertEqual(verified["deals"],[77])
        self.assertEqual(verified["position_id"],917)
        self.assertEqual(len(self.mt.sends),1,"One stage intent may never send twice")
        self.cmd["lots"]=0.06
        self.assertEqual(self.send()["state"],"unknown")
        self.assertEqual(len(self.mt.sends),1)
    def test_same_stage_plan_cannot_be_sent_under_second_id_even_after_first_fill(self):
        original=self.send()
        self.assertEqual(original["state"],"placed",original)
        self.history_fill()
        self.assertEqual(self.send()["state"],"filled")
        first_id=self.cmd["id"]
        self.cmd["id"]="tp-duplicate-"+"f"*24
        result=self.send()
        self.assertEqual(result["state"],"unknown",result)
        self.assertEqual(len(self.mt.sends),1)
        claim=self.ex.db.execute(
            "SELECT send_id FROM tp_stage_claims WHERE plan_id=?",
            (PLAN,)).fetchone()
        self.assertEqual(claim,(first_id,))
    def test_stage_claim_persists_after_gateway_restarts(self):
        self.assertEqual(self.send()["state"],"placed")
        self.history_fill()
        self.assertEqual(self.send()["state"],"filled")
        first_id=self.cmd["id"]
        self.ex.close()
        self.ex=BrokerExecutor(Path(self.tmp.name)/"gateway.sqlite")
        self.cmd["id"]="tp-restarted-"+"e"*24
        result=self.send()
        self.assertEqual(result["state"],"unknown",result)
        self.assertEqual(len(self.mt.sends),1)
        self.assertEqual(self.ex.db.execute(
            "SELECT send_id FROM tp_stage_claims WHERE plan_id=?",
            (PLAN,)).fetchone(),(first_id,))
    def test_orphaned_stage_claim_cannot_loop_or_emit_order(self):
        # Simulate a corrupted legacy database where a claim survives but
        # its matching sends row is gone. No recursive execute/retry allowed.
        self.ex.db.execute(
            "INSERT INTO tp_stage_claims(plan_id,send_id,created_ms) VALUES(?,?,1)",
            (PLAN,self.cmd["id"]))
        self.ex.db.commit()
        result=self.send()
        self.assertEqual(result["state"],"unknown",result)
        self.assertIn("claim collision",result["reason"])
        self.assertFalse(self.mt.sends)

    def test_incomplete_stage_fields_must_never_fall_back_to_full_close(self):
        for idx,missing in enumerate(("tp_stage_plan_id","tp_stage","expected_position_identifier",
                                                 "tp_target_price","expected_position_entry","expected_broker_tp")):
            self.cmd=stage_command()
            self.cmd["id"]=f"tp-incomplete-{idx}-"+"d"*24
            self.cmd.pop(missing)
            result=self.send()
            self.assertEqual(result["state"],"rejected",result)
            self.assertEqual(len(self.mt.sends),0)
    def test_null_stage_marker_cannot_fall_back_to_full_close(self):
        self.cmd=stage_command()
        for marker in ('tp_stage_plan_id','tp_stage','expected_position_identifier',
                       'tp_target_price','expected_position_entry','expected_broker_tp'):
            with self.subTest(marker=marker):
                cmd=stage_command()
                cmd.pop('tp_stage_plan_id',None)
                cmd.pop('tp_stage',None)
                cmd.pop('expected_position_identifier',None)
                cmd.pop('tp_target_price',None)
                cmd.pop('expected_position_entry',None)
                cmd.pop('expected_broker_tp',None)
                cmd[marker]=None
                self.cmd=cmd
                self.cmd['id']='tp-null-'+marker+'-'+'f'*20
                self.assertFalse(is_fast_path(self.cmd))
                result=self.send()
                self.assertEqual(result['state'],'rejected',result)
                self.assertIn('Incomplete TP stage marker',result['reason'])
                self.assertFalse(self.mt.sends)
    def test_lone_tp_price_marker_cannot_be_interpreted_as_full_close(self):
        self.cmd=stage_command()
        for key in ('tp_stage_plan_id','tp_stage','expected_position_identifier'):
            self.cmd.pop(key)
        self.assertFalse(is_fast_path(self.cmd))
        result=self.send()
        self.assertEqual(result['state'],'rejected',result)
        self.assertIn('Incomplete TP stage marker',result['reason'])
        self.assertFalse(self.mt.sends)
    def test_stage_replay_cannot_switch_identity_or_expected_size(self):
        self.assertEqual(self.send()["state"],"placed")
        self.cmd["tp_stage_plan_id"]="tp-plan-"+"0"*32
        self.assertEqual(self.send()["state"],"unknown")
        self.assertEqual(len(self.mt.sends),1)
        self.cmd["tp_stage_plan_id"]=PLAN
        self.cmd["tp_target_price"]=2000.0
        self.assertEqual(self.send()["state"],"unknown",
                         "A journalled TP price cannot be modified after placement")
        self.assertEqual(len(self.mt.sends),1)
    def test_tp2_and_tp3_keep_residual_on_broker_grid(self):
        self.p.volume=0.05
        tp2=stage_command(stage=2,held=0.05,lots=0.03)
        self.cmd=tp2
        self.assertEqual(self.send()["state"],"placed")
        self.assertAlmostEqual(self.mt.sends[0]["volume"],0.03)
        # TP3 cannot send with a different command while the TP2 order's
        # actual MT5 fills have not been reconciled.
        self.p.volume=0.02
        tp3=stage_command(stage=3,held=0.02,lots=0.02)
        self.cmd=tp3
        blocked=self.send()
        self.assertEqual(blocked["state"],"cancelled",blocked)
        self.assertEqual(len(self.mt.sends),1)
        # The block above is pre-journal: settle TP2 from its immutable Deal
        # with the same Gateway journal before retrying TP3.
        self.cmd=tp2
        self.history_fill(lots=0.03)
        settled=self.send()
        self.assertEqual(settled["state"],"filled",settled)
        self.assertTrue(settled["recovered_from_history"])
        prior=list(self.mt.deals)
        self.cmd=tp3
        def second_order(req):
            self.mt.sends.append(req)
            return N(retcode=10009,comment="broker",order=56,deal=78,
                     volume=req["volume"],price=req["price"])
        self.mt.order_send=second_order
        ready=self.send()
        self.assertEqual(ready["state"],"placed",ready)
        self.assertEqual(len(self.mt.sends),2)
        self.assertAlmostEqual(self.mt.sends[0]["volume"],0.03)
        self.assertAlmostEqual(self.mt.sends[1]["volume"],0.02)
        self.history_fill(lots=0.02,order=56,deal=78)
        self.mt.deals=prior+self.mt.deals
        final=self.send()
        self.assertEqual(final["state"],"filled",final)
        self.assertTrue(final["recovered_from_history"])
    def test_tp_price_must_be_fresh_tick_aligned_and_reached_at_preflight(self):
        cases=[
            ('missing',lambda c,m,p:c.pop('tp_target_price')),
            ('off_grid',lambda c,m,p:c.update(tp_target_price=1999.905)),
            ('below_entry',lambda c,m,p:c.update(tp_target_price=1999.0)),
            ('not_crossed',lambda c,m,p:c.update(tp_target_price=2000.5)),
            ('stale',lambda c,m,p:setattr(m.tick,'time_msc',clock_ms()-3500)),
            ('crossed_quote',lambda c,m,p:setattr(m.tick,'ask',1999.0)),
        ]
        for label,mutate in cases:
            with self.subTest(case=label):
                self.cmd=stage_command()
                self.cmd['id']='tp-price-'+label+'-'+'f'*24
                self.mt.tick.bid=2000.0
                self.mt.tick.ask=2000.1
                self.mt.tick.time_msc=clock_ms()
                mutate(self.cmd,self.mt,self.p)
                result=self.send()
                self.assertEqual(result['state'],'rejected',result)
                self.assertFalse(self.mt.sends)
    def test_tp_price_rechecked_after_order_check_before_any_broker_send(self):
        self.mt.on_check=lambda:setattr(self.mt.tick,'bid',1999.8)
        result=self.send()
        self.assertEqual(result['state'],'cancelled',result)
        self.assertIn('pre-send',result['reason'])
        self.assertEqual(len(self.mt.sends),0)
    def test_favourable_tp_quote_drift_also_denied_without_reusing_old_stage(self):
        # A fresh, independent fixture is required. The earlier test has
        # already atomically claimed its stage plan before the second gate;
        # retrying that same plan under a new ID correctly returns unknown.
        def advance_valid_spread():
            self.mt.tick.bid=2000.2
            self.mt.tick.ask=2000.3
        self.mt.on_check=advance_valid_spread
        result=self.send()
        self.assertEqual(result['state'],'cancelled',result)
        self.assertIn('quote moved',result['reason'])
        self.assertFalse(self.mt.sends)
    def test_sell_tp_uses_ask_and_requires_profit_side_price(self):
        self.p.type=1
        self.p.price_open=2000.5
        self.cmd['expected_position_side']='SELL'
        self.cmd['tp_target_price']=2000.2
        self.cmd['expected_position_entry']=2000.5
        self.mt.tick.bid=1999.9
        self.mt.tick.ask=2000.1
        result=self.send()
        self.assertEqual(result['state'],'placed',result)
        self.assertEqual(self.mt.sends[0]['type'],self.mt.ORDER_TYPE_BUY)
        self.assertEqual(self.mt.sends[0]['price'],2000.1)
    def test_late_broker_entry_or_original_tp_change_denies_send(self):
        for field,value in (('price_open',1998.5),('tp',2010.0)):
            with self.subTest(field=field):
                self.mt.on_check=lambda field=field,value=value:setattr(self.p,field,value)
                result=self.send()
                self.assertEqual(result['state'],'cancelled',result)
                self.assertIn('pre-send',result['reason'])
                self.assertEqual(len(self.mt.sends),0)
                # Fresh fixture-like plan identity after the first persisted
                # unsent claim; do not silently recycle another order ID.
                self.p.price_open=1999.5;self.p.tp=0.0
                self.cmd=stage_command()
                self.cmd['id']='stage-identity-'+field+'-'+'0'*20
                self.cmd['tp_stage_plan_id']='tp-plan-'+(
                    'd'*32 if field=='price_open' else 'e'*32)
        self.mt.on_check=None
    def test_late_broker_volume_change_denies_send_even_after_first_authorization(self):
        self.mt.on_check=lambda:setattr(self.p,"volume",0.08)
        res=self.send()
        self.assertEqual(res["state"],"cancelled",res)
        self.assertFalse(self.mt.sends)
        self.assertIn("pre-send",res["reason"])
    def test_forged_broker_deal_cannot_complete_acknowledged_stage(self):
        ack=self.send()
        self.assertEqual(ack["state"],"placed")
        for label,change in (
            ("wrong_order",dict(order=999)),
            ("missing_order_but_same_magic_comment",dict(order=0)),
            ("wrong_position",dict(position=918)),
            ("wrong_entry",dict(entry=0)),
            ("close_by_is_not_a_market_tp",dict(entry=3)),
            ("wrong_direction",dict(side=0)),
            ("wrong_owner",dict(magic=999)),
            ("underfill",dict(lots=0.03)),
        ):
            with self.subTest(label=label):
                self.history_fill(**change)
                pending=self.send()
                self.assertNotEqual(pending["state"],"filled",pending)
                self.assertEqual(len(self.mt.sends),1)
        self.history_fill()
        confirmed=self.send()
        self.assertEqual(confirmed["state"],"filled",confirmed)
        self.assertTrue(confirmed["recovered_from_history"])
        self.assertEqual(len(self.mt.sends),1)
    def test_missing_mt5_order_ticket_never_turns_a_stage_into_a_fill(self):
        ack=self.send()
        self.assertEqual(ack["state"],"placed",ack)
        # Same comment, magic, account, position, direction and total lots
        # cannot substitute for an actual MT5 Deal.order ticket.
        self.history_fill(order=0)
        result=self.send()
        self.assertEqual(result["state"],"placed",result)
        self.assertFalse(result.get("recovered_from_history",False))
        self.assertEqual(len(self.mt.sends),1)
        # Restart must preserve the single-shot claim while evidence is missing.
        self.ex.close()
        self.ex=BrokerExecutor(Path(self.tmp.name)/"gateway.sqlite")
        result=self.send()
        self.assertEqual(result["state"],"placed",result)
        self.assertEqual(len(self.mt.sends),1)
        self.history_fill(order=55)
        result=self.send()
        self.assertEqual(result["state"],"filled",result)
        self.assertTrue(result["recovered_from_history"])
        self.assertEqual(len(self.mt.sends),1)
    def test_zero_magic_exit_only_accepted_with_exact_order_and_physical_position(self):
        self.assertEqual(self.send()["state"],"placed")
        self.history_fill(magic=0,order=999)
        self.assertEqual(self.send()["state"],"placed",
                         "A cleared magic cannot bypass the exact acknowledged order")
        self.history_fill(magic=0,position=918)
        self.assertEqual(self.send()["state"],"placed",
                         "A cleared magic cannot bypass the bound physical position")
        self.history_fill(magic=0)
        result=self.send()
        self.assertEqual(result["state"],"filled",result)
        self.assertTrue(result["recovered_from_history"])
        self.assertEqual(len(self.mt.sends),1)
    def test_filled_ack_cannot_be_overwritten_by_zero_fill_terminal_history(self):
        self.assertEqual(self.send()["state"],"placed")
        self.ex._terminal_partial_order=lambda *_:dict(confirmed=True,ticket=55,
            state="canceled",volume_initial=0.05,volume_current=0.05)
        result=self.send()
        self.assertEqual(result["state"],"placed",result)
        self.assertIn("no cancel/no resend",result["reason"])
        self.assertEqual(len(self.mt.sends),1)
        self.history_fill()
        self.assertEqual(self.send()["state"],"filled")
        self.assertEqual(len(self.mt.sends),1)
    def test_tp_done_ack_with_missing_deal_ticket_stays_unresolved_on_conflicting_zero_fill(self):
        self.mt.reply_deal=0
        ack=self.send()
        self.assertEqual(ack["state"],"placed",ack)
        self.assertEqual(ack["retcode"],10009)
        self.ex._terminal_partial_order=lambda *_:dict(confirmed=True,ticket=55,
            state="canceled",volume_initial=0.05,volume_current=0.05)
        unresolved=self.send()
        self.assertEqual(unresolved["state"],"placed",unresolved)
        self.assertIn("no cancel/no resend",unresolved["reason"])
        self.assertEqual(len(self.mt.sends),1)
        self.history_fill()
        confirmed=self.send()
        self.assertEqual(confirmed["state"],"filled",confirmed)
        self.assertTrue(confirmed["recovered_from_history"])
        self.assertEqual(len(self.mt.sends),1)
    def test_genuine_zero_fill_placed_order_can_be_cancelled_without_resend(self):
        self.mt.code=10008
        self.mt.reply_deal=0
        self.assertEqual(self.send()["state"],"placed")
        self.ex._terminal_partial_order=lambda *_:dict(confirmed=True,ticket=55,
            state="canceled",volume_initial=0.05,volume_current=0.05)
        result=self.send()
        self.assertEqual(result["state"],"cancelled",result)
        self.assertEqual(len(self.mt.sends),1)
        self.assertEqual(self.send()["state"],"cancelled")
        self.assertEqual(len(self.mt.sends),1)
    def test_stage_done_ack_restart_waits_for_history_and_never_resends(self):
        self.assertEqual(self.send()["state"],"placed")
        self.ex.close()
        self.ex=BrokerExecutor(Path(self.tmp.name)/"gateway.sqlite")
        self.assertEqual(self.send()["state"],"placed")
        self.assertEqual(len(self.mt.sends),1)
        self.history_fill()
        confirmed=self.send()
        self.assertEqual(confirmed["state"],"filled",confirmed)
        self.assertTrue(confirmed["recovered_from_history"])
        self.assertEqual(len(self.mt.sends),1)
    def test_no_retries_after_unknown_broker_result(self):
        self.mt.raise_send=True
        result=self.send()
        self.assertEqual(result["state"],"unknown",result)
        self.mt.raise_send=False
        _=self.send()
        self.assertEqual(len(self.mt.sends),1)
    def test_fail_closed_bad_sizing_identity_account_side_or_margin(self):
        mutations=[
            lambda c,m,p:c.update(lots=0.009),
            lambda c,m,p:c.update(lots=0.055),
            lambda c,m,p:c.update(lots=0.095),
            lambda c,m,p:c.update(lots=0.11),
            lambda c,m,p:c.update(expected_position_volume=0.11),
            lambda c,m,p:c.update(expected_position_identifier=999),
            lambda c,m,p:c.update(expected_position_side="SELL"),
            lambda c,m,p:c.update(tp_stage=4),
            lambda c,m,p:c.update(tp_stage_plan_id="unknown-plan"),
            lambda c,m,p:c.update(expected_position_entry=1998.0),
            lambda c,m,p:setattr(p,"price_open",1998.0),
            lambda c,m,p:setattr(p,"tp",2050.0),
            lambda c,m,p:c.update(expected_broker_tp=2000.0),
            lambda c,m,p:c.update(target="live"),
            lambda c,m,p:setattr(m.account,"margin_mode",0),
            lambda c,m,p:setattr(m.account,"trade_mode",2),
            lambda c,m,p:setattr(p,"identifier",918),
            lambda c,m,p:c.update(manual_close=True),
        ]
        for idx,mutate in enumerate(mutations):
            with self.subTest(index=idx):
                self.mt.account.margin_mode=2
                self.mt.account.trade_mode=0
                self.p.identifier=917;self.p.volume=0.1
                self.p.price_open=1999.5;self.p.tp=0.0
                self.cmd=stage_command()
                self.cmd["id"]=f"tp-negative-{idx}-"+"c"*20
                mutate(self.cmd,self.mt,self.p)
                result=self.send()
                self.assertIn(result["state"],("rejected","cancelled"),result)
                self.assertFalse(self.mt.sends)
    def test_selected_live_stage_contract_matches_real_mode_without_rewriting_account(self):
        self.mt.account.trade_mode=2
        self.cmd["target"]="live"
        original=dict(self.cmd)
        volume,side=stage_volume(self.cmd,self.mt.account,self.mt,self.p,self.mt.info,
                                 self.mt.tick,clock_ms())
        self.assertEqual((volume,side),(0.05,-1))
        self.assertEqual(self.cmd,original)
        self.cmd["target"]="demo"
        with self.assertRaisesRegex(ValueError,"account mode"):
            stage_volume(self.cmd,self.mt.account,self.mt,self.p,self.mt.info,
                         self.mt.tick,clock_ms())
        self.cmd["target"]="live"
        self.mt.account.trade_mode=0
        with self.assertRaisesRegex(ValueError,"account mode"):
            stage_volume(self.cmd,self.mt.account,self.mt,self.p,self.mt.info,
                         self.mt.tick,clock_ms())
        self.assertFalse(self.mt.sends)

    def test_final_authorization_refusal_stops_before_send(self):
        self.assertEqual(self.send(lambda _:False)["state"],"cancelled")
        self.assertFalse(self.mt.sends)
    def test_helper_does_not_round_or_mutate_original_request(self):
        before=dict(self.cmd)
        qty,side=stage_volume(self.cmd,self.mt.account,self.mt,self.p,self.mt.info,
                              self.mt.tick,clock_ms())
        self.assertEqual((qty,side),(0.05,-1))
        self.assertEqual(before,self.cmd)
        self.assertNotIn("request",self.cmd)


if __name__=="__main__":
    unittest.main()
