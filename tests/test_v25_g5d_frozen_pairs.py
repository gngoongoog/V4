"""G5-D adversarial fixtures: never a real MT5 Demo win-rate experiment."""
import importlib.util
import json
from pathlib import Path
import sqlite3
import unittest


SPEC = importlib.util.spec_from_file_location(
    "g5d_audit", Path(__file__).resolve().parents[1] /
    "scripts" / "audit_v25_g5d_frozen_pairs.py"
)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)

BAR = 1_760_000_000


def sample(bar=BAR):
    observed = (bar + 60) * 1000 + 500
    opened = observed - 250
    closed = opened + 12_000
    decision = {
        "frozen": {"side": 1, "ready": True, "key": f"f-{bar}"},
        "adaptive": {"side": -1, "ready": True, "key": f"a-{bar}"},
    }
    p = {
        "policy": AUDIT.PAPER_POLICY, "bar_time": bar, "observed_ms": opened,
        "source": "MT5 Demo BID/ASK paired prospective quote; no order_send",
        "lots": 0.01, "tick": 0.01, "profit_tick": 1.0, "loss_tick": 1.0,
        "commission_per_lot": 7.0, "slippage_ticks": 1.0,
        "entry_bid": 2000.0, "entry_ask": 2000.02,
        "indicative_contract": {"status": "pass"},
    }
    def leg(side, entry, exit_price, name):
        ticks = ((exit_price - entry) * side) / 0.01 - 2
        gross = ticks * 0.01
        commission = 7 * 0.01
        net = gross - commission
        return {
            "side": side, "state": "closed", "entry": entry,
            "stop_distance": 0.50, "target_distance": 0.50, "hold_ms": 10_000,
            "signal_key": f"{name}-{bar}", "last_quote_ms": closed,
            "max_gap_ms": 5000,
            "result": {
                "exit_ms": closed, "exit": exit_price, "reason": "time",
                "gross": gross, "commission_assumed": commission,
                "net_paper": net, "net_per_lot": net / 0.01,
                "max_quote_gap_ms": 5000, "gap_risk": False,
                "provenance": AUDIT.PROVENANCE,
            },
        }
    p["frozen"] = leg(1, 2000.02, 2000.12, "f")
    p["adaptive"] = leg(-1, 2000.0, 1999.85, "a")
    return (observed, opened, closed, decision, p)


class G5DTests(unittest.TestCase):
    def setUp(self):
        self.db = sqlite3.connect(":memory:")
        self.db.executescript("""
            CREATE TABLE frozen_coordinator_runs (
                id TEXT PRIMARY KEY,context TEXT,mode TEXT,created_ms INTEGER,
                snapshot TEXT,active INTEGER);
            CREATE TABLE frozen_coordinator_decisions (
                run_id TEXT,bar_time INTEGER,observed_ms INTEGER,frozen TEXT,adaptive TEXT,
                PRIMARY KEY(run_id,bar_time));
            CREATE TABLE frozen_coordinator_trade_pairs (
                run_id TEXT,bar_time INTEGER,opened_ms INTEGER,closed_ms INTEGER,
                state TEXT,payload TEXT,PRIMARY KEY(run_id,bar_time));
        """)
        self.created = (BAR - 600) * 1000
        self.db.execute("INSERT INTO frozen_coordinator_runs VALUES(?,?,?,?,?,1)",
                        ("run-demo", '["broker",123,"XAUUSD","USD"]', "expert",
                         self.created, json.dumps({
                             "policy": AUDIT.POLICY, "weights_frozen": True,
                             "broker_permission": False, "created_ms": self.created,
                         })))

    def tearDown(self):
        self.db.close()

    def add(self, bar=BAR, mutate=None, state="complete", run_id="run-demo"):
        observed, opened, closed, decision, p = sample(bar)
        if mutate:
            mutate(decision, p)
        self.db.execute("INSERT INTO frozen_coordinator_decisions VALUES(?,?,?,?,?)",
                        (run_id, bar, observed, json.dumps(decision["frozen"]),
                         json.dumps(decision["adaptive"])))
        self.db.execute("INSERT INTO frozen_coordinator_trade_pairs VALUES(?,?,?,?,?,?)",
                        (run_id, bar, opened, closed, state, json.dumps(p)))

    def test_clean_pair_is_descriptive_not_broker_pnl(self):
        self.add()
        r = AUDIT.audit(self.db, "run-demo")
        self.assertEqual(r["eligible_pairs"], 1)
        self.assertEqual(r["adaptive_minus_frozen_mean_paper_net_per_lot"], 5.0)
        self.assertEqual(r["status"], "insufficient_prospective_pairs")
        self.assertIsNone(r["broker_pnl"])
        self.assertFalse(r["live_gate_pass"])
        self.assertFalse(r["model_promotion_allowed"])

    def test_mutated_costs_and_forged_predictions_are_rejected(self):
        self.add(BAR, lambda d,p: p["adaptive"]["result"].update(
            {"commission_assumed": 0, "net_paper": 9999, "net_per_lot": 999900}))
        self.add(BAR + 60, lambda d,p: d["frozen"].update({"key": "post-hoc-changed"}))
        self.add(BAR + 120, lambda d,p: p["adaptive"]["result"].update(
            {"max_quote_gap_ms": 23000, "gap_risk": False}))
        self.add(BAR + 180, lambda d,p: p["indicative_contract"].update({"status": "unknown"}))
        self.add(BAR + 240, lambda d,p: p["adaptive"]["result"].update({"exit_ms": 9_000_000_000_000}))
        self.add(BAR + 300, lambda d,p: p["frozen"]["result"].update(
            {"provenance": "MT5 confirmed real booked net PnL"}))
        self.add(BAR + 360, lambda d,p: p["adaptive"]["result"].update({"reason": "target"}))
        self.add(BAR + 420, lambda d,p: p["frozen"].update({"max_gap_ms": 9000}))
        r = AUDIT.audit(self.db, "run-demo")
        self.assertEqual(r["eligible_pairs"], 0)
        self.assertEqual(sum(r["rejected_by_reason"].values()), 8)
        self.assertIsNone(r["adaptive_minus_frozen_mean_paper_net_per_lot"])

    def test_incomplete_or_missing_ex_ante_decision_not_fabricated(self):
        self.add(state="open")
        observed, opened, closed, _, p = sample(BAR + 60)
        self.db.execute("INSERT INTO frozen_coordinator_trade_pairs VALUES(?,?,?,?,?,?)",
                        ("run-demo", BAR + 60, opened, closed, "complete", json.dumps(p)))
        r = AUDIT.audit(self.db, "run-demo")
        self.assertEqual(r["eligible_pairs"], 0)
        self.assertEqual(r["rejected_by_reason"]["not_complete"], 1)
        self.assertEqual(r["rejected_by_reason"]["missing_pre_outcome_decision"], 1)

    def test_chronological_daily_blocks_and_account_isolation(self):
        for i in range(35):
            self.add(BAR + i * 86_400)
        self.db.execute("INSERT INTO frozen_coordinator_runs VALUES(?,?,?,?,?,1)",
                        ("other-run", '["other",55,"XAUUSD","USD"]', "expert",
                         self.created, json.dumps({
                             "policy": AUDIT.POLICY, "weights_frozen": True,
                             "broker_permission": False, "created_ms": self.created,
                         })))
        r = AUDIT.audit(self.db, "run-demo")
        self.assertEqual(r["eligible_pairs"], 35)
        self.assertEqual(r["distinct_utc_days"], 35)
        self.assertEqual(r["status"], "exploratory_quote_paper_only")
        self.assertEqual(r["adaptive_minus_frozen_daily_block_bootstrap_95pct"], [5.0, 5.0])
        self.assertFalse(r["independent_oos"])
        other = AUDIT.audit(self.db, "other-run")
        self.assertEqual(other["eligible_pairs"], 0)
        self.assertNotEqual(r["account_scope_hash"], other["account_scope_hash"])

    def test_future_dated_completion_fails_even_with_matching_costs(self):
        self.add()
        r = AUDIT.audit(self.db, "run-demo", now_ms=(BAR + 60) * 1000 + 600)
        self.assertEqual(r["eligible_pairs"], 0)
        self.assertEqual(r["rejected_by_reason"]["missing_same_cycle_entry_proof"], 1)

    def test_forged_early_time_exit_is_rejected(self):
        self.add(BAR, lambda d,p: [
            p["frozen"].update({"hold_ms": 100_000}),
            p["adaptive"].update({"hold_ms": 100_000}),
        ])
        r = AUDIT.audit(self.db, "run-demo")
        self.assertEqual(r["eligible_pairs"], 0)
        self.assertEqual(r["rejected_by_reason"]["exit_policy_reason_mismatch"], 1)

    def test_invalid_snapshot_never_qualifies(self):
        self.db.execute("UPDATE frozen_coordinator_runs SET snapshot=? WHERE id='run-demo'",
                        (json.dumps({"policy": AUDIT.POLICY, "weights_frozen": True,
                                     "broker_permission": True,
                                     "created_ms": self.created}),))
        self.add()
        r = AUDIT.audit(self.db, "run-demo")
        self.assertEqual(r["status"], "invalid_frozen_run")
        self.assertFalse(r["model_promotion_allowed"])


if __name__ == "__main__":
    unittest.main()
