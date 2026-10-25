"""G6-B: executed-demo evidence requires exact prior signal and fully booked MT5 deals."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sqlite3
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import audit_v25_g6a_preregistered_quotes as g6a
import audit_v25_g6b_executed_demo_arm as study

spec = importlib.util.spec_from_file_location(
    "g6b_g5d_fixtures", ROOT/"tests"/"test_v25_g5d_frozen_pairs.py")
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
BAR = base.BAR
REGISTERED = (BAR-300)*1000
FUTURE = BAR+1800
NOW = (BAR+200*86400)*1000
CONTEXT = '["broker",123,"XAUUSD","USD"]'


class ExecutedBrokerEvidence(unittest.TestCase):
    def setUp(self):
        self.fx = base.G5DTests()
        self.fx.setUp()
        self.db = self.fx.db
        self.db.executescript("""
          CREATE TABLE broker_commands(
            id TEXT PRIMARY KEY,context TEXT,brain TEXT,kind TEXT,
            created_ms INTEGER,state TEXT,payload TEXT,result TEXT);
          CREATE TABLE broker_deals(
            context TEXT,ticket INTEGER,position_id INTEGER,brain TEXT,payload TEXT,
            PRIMARY KEY(context,ticket));
          CREATE TABLE coordinator_broker_samples(
            source TEXT,position_id INTEGER,context TEXT,brain TEXT,
            closed_ms INTEGER,payload TEXT,PRIMARY KEY(source,position_id));
        """)
        self.manifest = g6a.register(self.db, "run-demo", now_ms=REGISTERED)

    def tearDown(self):
        self.fx.tearDown()

    def add(self, bar=FUTURE, *, position_id=None):
        self.fx.add(bar)
        observed, opened, closed, _, _ = base.sample(bar)
        pos = position_id or bar
        deal = pos+10_000
        cmd_id = f"cmd-{pos}"
        signal = {"key":f"a-{bar}","side":-1,"bar_time":bar,
                  "coordination":{"trading_mode":"expert"}}
        command = {"id":cmd_id,"brain":"system","signal":signal,
                   "learning":{"trading_mode":"expert"}}
        self.db.execute("INSERT INTO broker_commands VALUES(?,?,'system','open',?,'observed',?,?)",
                        (cmd_id,CONTEXT,observed+500,json.dumps(command),
                         json.dumps({"deal":deal})))
        for ticket, entry, kind, profit, commission, when in (
            (deal,0,1,0.0,-0.2,observed+700),
            (deal+1,1,0,2.0,-0.3,closed)):
            p = {"ticket":ticket,"position_id":pos,"symbol":"XAUUSD",
                 "type":kind,"entry":entry,"volume":0.1,"time_msc":when,
                 "profit":profit,"commission":commission,"swap":0.0,"fee":0.0}
            self.db.execute("INSERT INTO broker_deals VALUES(?,?,?,'system',?)",
                            (CONTEXT,ticket,pos,json.dumps(p)))
        receipt = {
            "evidence_source":"mt5_demo_broker","context":CONTEXT,
            "brain":"system","trading_mode":"expert","position_id":pos,
            "closed_ms":closed,"open_command_id":cmd_id,"open_deal":deal,
            "executed_side":-1,"gross":2.0,"commission":-0.5,
            "swap":0.0,"fee":0.0,"net":1.5,"closed_volume":0.1,
            "net_per_lot":15.0,"risk_money":5.0,"r_multiple":0.3,
            "reward":study.math.tanh(0.3)}
        self.db.execute("INSERT INTO coordinator_broker_samples VALUES(?,? ,?,'system',?,?)",
                        (f"source-{pos}",pos,CONTEXT,closed,json.dumps(receipt)))
        return pos, deal

    def test_exact_demo_deals_have_descriptive_single_arm_not_paired_edge(self):
        self.add()
        r = study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],1)
        self.assertEqual(r["adaptive_executed_demo_booked_net"],1.5)
        self.assertEqual(r["adaptive_executed_demo_net_per_lot_mean"],15.0)
        self.assertEqual(r["adaptive_executed_demo_costs"],-0.5)
        self.assertIsNone(r["paired_executed_broker_delta"])
        self.assertIsNone(r["frozen_executed_broker_pnl"])
        self.assertFalse(r["independent_oos"])
        self.assertFalse(r["automatic_model_promotion"])

    def test_old_broker_sample_never_enters_new_holdout_denominator(self):
        self.add()
        self.db.execute(
            "INSERT INTO coordinator_broker_samples VALUES('legacy',9,?,'system',?,?)",
            (CONTEXT,REGISTERED-1000,json.dumps({"net":9999.0})))
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["excluded_pre_holdout_broker_records"],1)
        self.assertEqual(r["broker_records_examined"],1)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],1)
        self.assertEqual(r["adaptive_executed_demo_booked_net"],1.5)

    def test_missing_learning_receipt_cannot_hide_open_intent_from_denominator(self):
        self.add()
        self.db.execute("DELETE FROM coordinator_broker_samples")
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["preregistered_expert_open_intents"],1)
        self.assertEqual(r["predeclared_adaptive_open_intents"],1)
        self.assertEqual(r["predeclared_intents_without_verified_full_close"],1)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],0)
        self.assertEqual(r["status"],"incomplete_prospective_broker_outcome_capture")
        self.assertIsNone(r["adaptive_executed_demo_booked_net"])
        self.assertFalse(r["all_post_holdout_expert_intents_accounted_for"])

    def test_open_intent_with_wrong_prior_signal_is_visible_as_unattributed(self):
        self.add()
        self.db.execute(
            "UPDATE broker_commands SET payload=json_set(payload,'$.signal.key','late-changed')")
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["preregistered_expert_open_intents"],1)
        self.assertEqual(r["expert_open_intents_missing_prior_adaptive_match"],1)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],0)
        self.assertFalse(r["all_post_holdout_expert_intents_accounted_for"])

    def test_open_intent_without_any_decision_must_not_improve_report(self):
        self.add()
        bar=FUTURE+3600
        opened=(bar+60)*1000
        command={"id":"stray","brain":"system",
                 "learning":{"trading_mode":"expert"},
                 "signal":{"bar_time":bar,"key":"stray-signal","side":1}}
        self.db.execute("INSERT INTO broker_commands VALUES("
            "'stray',?,'system','open',?,'unknown',?,NULL)",
            (CONTEXT,opened,json.dumps(command)))
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],1)
        self.assertEqual(r["expert_open_intents_missing_prior_adaptive_match"],1)
        self.assertEqual(r["status"],"incomplete_prospective_broker_outcome_capture")
        self.assertEqual(r["adaptive_executed_demo_win_rate"],1.0)
        self.assertEqual(r["win_rate_scope"],
                         "matched_closed_positions_only_not_all_open_intents")

    def test_missing_exit_deal_never_creates_a_broker_win(self):
        pos,deal = self.add()
        self.db.execute("DELETE FROM broker_deals WHERE ticket=?", (deal+1,))
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],0)
        self.assertEqual(r["rejected_by_reason"]["broker_entry_exit_not_fully_reconciled"],1)

    def test_small_partial_close_never_passes_rust_volume_geometry(self):
        pos,deal=self.add()
        self.db.execute(
            "UPDATE broker_deals SET payload=json_set(payload,'$.volume',0.0999998) "
            "WHERE ticket=?", (deal+1,))
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],0)
        self.assertEqual(r["rejected_by_reason"]["broker_entry_exit_not_fully_reconciled"],1)

    def test_wrong_symbol_in_deal_is_never_this_account_trade(self):
        pos,deal=self.add()
        self.db.execute(
            "UPDATE broker_deals SET payload=json_set(payload,'$.symbol','EURUSD') "
            "WHERE ticket=?", (deal+1,))
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],0)
        self.assertEqual(r["rejected_by_reason"]["broker_deal_geometry_unknown"],1)

    def test_backdated_mt5_entry_cannot_pretend_to_be_new_study_execution(self):
        pos,deal=self.add()
        command=self.db.execute(
            "SELECT created_ms FROM broker_commands WHERE id=?",
            (f"cmd-{pos}",)).fetchone()[0]
        self.db.execute(
            "UPDATE broker_deals SET payload=json_set(payload,'$.time_msc',?) "
            "WHERE ticket=?",(command-1,deal))
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],0)
        self.assertEqual(r["predeclared_intents_without_verified_full_close"],1)
        self.assertEqual(r["rejected_by_reason"]["broker_entry_exit_not_fully_reconciled"],1)
        self.assertEqual(r["status"],"incomplete_prospective_broker_outcome_capture")

    def test_invalid_extra_receipt_blocks_false_complete_study_census(self):
        for i in range(32):
            self.add(FUTURE+i*6*3600)
        self.db.execute(
            "INSERT INTO coordinator_broker_samples VALUES("
            "'forged',99999,?,'system',?,?)",
            (CONTEXT,(FUTURE+60)*1000,json.dumps({"net":999999})))
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],32)
        self.assertFalse(r["all_post_holdout_expert_intents_accounted_for"])
        self.assertEqual(r["status"],"incomplete_prospective_broker_outcome_capture")

    def test_missing_cost_component_fails_closed(self):
        pos,deal=self.add()
        self.db.execute(
            "UPDATE broker_deals SET payload=json_remove(payload,'$.fee') WHERE ticket=?",
            (deal+1,))
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],0)
        self.assertEqual(r["rejected_by_reason"]["missing_nonfinite_cost"],1)

    def test_gross_fee_mismatch_and_reward_poisoning_both_fail(self):
        pos,_=self.add()
        self.db.execute(
            "UPDATE coordinator_broker_samples SET payload=json_set(payload,'$.reward',0.99)",
            ())
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],0)
        self.assertEqual(r["rejected_by_reason"]["incorrect_broker_cost_or_reward_lineage"],1)

    def test_correct_money_cannot_link_to_wrong_signal_key(self):
        pos,_=self.add()
        self.db.execute(
            "UPDATE broker_commands SET payload=json_set(payload,'$.signal.key','wrong')",
            ())
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],0)
        self.assertEqual(r["rejected_by_reason"]["booked_trade_cannot_be_matched_to_adaptive_arm"],1)

    def test_correct_money_cannot_link_to_wrong_account(self):
        self.add()
        self.db.execute(
            "UPDATE coordinator_broker_samples SET payload=json_set(payload,'$.context','other')",
            ())
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],0)
        self.assertEqual(r["rejected_by_reason"]["broker_account_mode_or_position_mismatch"],1)

    def test_duplicate_broker_position_is_not_double_credited(self):
        pos,_=self.add()
        self.db.execute("INSERT INTO coordinator_broker_samples "
                        "SELECT 'duplicate',position_id,context,brain,closed_ms,payload "
                        "FROM coordinator_broker_samples")
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],0)
        self.assertEqual(r["rejected_by_reason"]["duplicate_broker_position_attribution"],2)

    def test_missing_broker_deal_table_does_not_invent_booked_pnl(self):
        self.db.execute("DROP TABLE broker_deals")
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["status"],"broker_tables_unavailable")
        self.assertIsNone(r["adaptive_executed_demo_booked_net"])

    def test_many_broker_closes_are_still_not_frozen_executed_A_B(self):
        for i in range(32):
            self.add(FUTURE+i*6*3600)
        r=study.audit(self.db,self.manifest,now_ms=NOW)
        self.assertEqual(r["exactly_linked_adaptive_broker_positions"],32)
        self.assertEqual(r["status"],"single_executed_adaptive_arm_descriptive_only")
        self.assertEqual(r["adaptive_executed_demo_booked_net"],48.0)
        self.assertIsNone(r["paired_executed_broker_delta"])
        self.assertEqual(r["broker_fills_for_frozen_comparator"],0)
        self.assertFalse(r["independent_oos"])


if __name__ == "__main__":
    unittest.main()
