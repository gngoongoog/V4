"""Offline DEMO-only TP partial-close Gateway regressions.

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
from execution import BrokerExecutor, is_fast_path
from test_broker_execution import Broker, command
from tp_partial_close import stage_volume

PLAN="tp-plan-"+"a"*32


def stage_command(stage=1,held=0.1,lots=0.05):
    c=command()
    c.update(id="tp-req-"+str(stage)+"-"+"b"*24,kind="close",position=91,
        brain="gann",magic=410601,lots=lots,signal=None,
        tp_stage_plan_id="tp-plan-"+{1:"a",2:"b",3:"c"}.get(stage,"a")*32,
        tp_stage=stage,
        expected_position_identifier=917,
        expected_position_volume=held,expected_position_side="BUY")
    return c


class StageDemoTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(prefix="bt-stage-demo-")
        self.ex=BrokerExecutor(Path(self.tmp.name)/"gateway.sqlite")
        self.mt=Broker()
        self.p=N(ticket=91,identifier=917,magic=410601,symbol="XAUUSD",
            volume=0.1,type=0,profit=8.)
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
        for idx,missing in enumerate(("tp_stage_plan_id","tp_stage","expected_position_identifier")):
            self.cmd=stage_command()
            self.cmd["id"]=f"tp-incomplete-{idx}-"+"d"*24
            self.cmd.pop(missing)
            result=self.send()
            self.assertEqual(result["state"],"rejected",result)
            self.assertEqual(len(self.mt.sends),0)
    def test_stage_replay_cannot_switch_identity_or_expected_size(self):
        self.assertEqual(self.send()["state"],"placed")
        self.cmd["tp_stage_plan_id"]="tp-plan-"+"0"*32
        self.assertEqual(self.send()["state"],"unknown")
        self.assertEqual(len(self.mt.sends),1)
    def test_tp2_and_tp3_keep_residual_on_broker_grid(self):
        self.p.volume=0.05
        self.cmd=stage_command(stage=2,held=0.05,lots=0.03)
        self.assertEqual(self.send()["state"],"placed")
        self.assertAlmostEqual(self.mt.sends[0]["volume"],0.03)
        self.p.volume=0.02
        self.cmd=stage_command(stage=3,held=0.02,lots=0.02)
        self.assertEqual(self.send()["state"],"placed")
        self.assertAlmostEqual(self.mt.sends[1]["volume"],0.02)
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
            ("wrong_position",dict(position=918)),
            ("wrong_entry",dict(entry=0)),
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
                self.cmd=stage_command()
                self.cmd["id"]=f"tp-negative-{idx}-"+"c"*20
                mutate(self.cmd,self.mt,self.p)
                result=self.send()
                self.assertIn(result["state"],("rejected","cancelled"),result)
                self.assertFalse(self.mt.sends)
    def test_final_authorization_refusal_stops_before_send(self):
        self.assertEqual(self.send(lambda _:False)["state"],"cancelled")
        self.assertFalse(self.mt.sends)
    def test_helper_does_not_round_or_mutate_original_request(self):
        before=dict(self.cmd)
        qty,side=stage_volume(self.cmd,self.mt.account,self.mt,self.p,self.mt.info)
        self.assertEqual((qty,side),(0.05,-1))
        self.assertEqual(before,self.cmd)
        self.assertNotIn("request",self.cmd)


if __name__=="__main__":
    unittest.main()
