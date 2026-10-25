"""G7-B adversarial tests: exact source -> Demo command -> fully closed MT5."""
import hashlib
import json
import math
import sqlite3
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from g7_core_source_audit import PROTOCOL, sha
from g7_demo_broker_lineage import audit

CTX = json.dumps(["Demo", 17, "XAUUSD", "USD"], separators=(",", ":"))
RUN = "g7-run"
BAR = 180
OBS = 241000
OPEN = 701
POS = 900
CMD = "command-g7"
CLOSED = 245000
ADMITTED = 250000


def as_json(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"))


def make_db():
    db = sqlite3.connect(":memory:")
    db.row_factory = sqlite3.Row
    db.executescript("""
        CREATE TABLE frozen_coordinator_runs(
          id TEXT PRIMARY KEY,context TEXT,mode TEXT,created_ms INTEGER,snapshot TEXT,active INTEGER);
        CREATE TABLE frozen_coordinator_decisions(
          run_id TEXT,bar_time INTEGER,observed_ms INTEGER,
          frozen TEXT,adaptive TEXT,PRIMARY KEY(run_id,bar_time));
        CREATE TABLE g7_pair_registrations(
          run_id TEXT PRIMARY KEY,context TEXT,mode TEXT,registered_ms INTEGER,
          cohort_start_ms INTEGER,frozen_snapshot_sha256 TEXT,protocol TEXT);
        CREATE TABLE g7_paired_decision_evidence(
          run_id TEXT,bar_time INTEGER,observed_ms INTEGER,payload TEXT,payload_sha256 TEXT,
          PRIMARY KEY(run_id,bar_time));
        CREATE TABLE broker_commands(
          id TEXT PRIMARY KEY,context TEXT,brain TEXT,kind TEXT,
          created_ms INTEGER,state TEXT,payload TEXT,result TEXT);
        CREATE TABLE broker_deals(
          context TEXT,ticket INTEGER,position_id INTEGER,brain TEXT,payload TEXT,
          PRIMARY KEY(context,ticket));
        CREATE TABLE coordinator_broker_samples(
          source TEXT,position_id INTEGER,context TEXT,brain TEXT,closed_ms INTEGER,
          payload TEXT,PRIMARY KEY(source,position_id));
        CREATE TABLE coordinator_broker_admissions(
          source TEXT,position_id INTEGER,context TEXT,brain TEXT,closed_ms INTEGER,
          admitted_ms INTEGER,payload_sha256 TEXT,protocol TEXT,
          PRIMARY KEY(source,position_id));
    """)
    snap = as_json({"policy":"frozen"})
    frozen = {"ready":True,"side":-1,"key":"bar:180","bar_time":BAR}
    adaptive = {"ready":True,"side":1,"key":"bar:180","bar_time":BAR}
    f,a = as_json(frozen),as_json(adaptive)
    db.execute("INSERT INTO frozen_coordinator_runs VALUES (?,?,?,?,?,?)",
               (RUN,CTX,"expert",123000,snap,1))
    db.execute("INSERT INTO g7_pair_registrations VALUES (?,?,?,?,?,?,?)",
               (RUN,CTX,"expert",123000,123001,sha(snap),PROTOCOL))
    db.execute("INSERT INTO frozen_coordinator_decisions VALUES (?,?,?,?,?)",
               (RUN,BAR,OBS,f,a))
    evidence = {
        "protocol":PROTOCOL,"run_id":RUN,"context":CTX,"mode":"expert",
        "registered_ms":123000,"cohort_start_ms":123001,
        "frozen_checkpoint_sha256":sha(snap),
        "bar_time":BAR,"observed_ms":OBS,
        "frozen_signal_sha256":sha(f),"adaptive_signal_sha256":sha(a),
        "frozen":frozen,"adaptive":adaptive,
        "execution":"decision_only_neither_arm_broker_executed",
        "booked_pnl":None,"source":"Core/frozen_coordinator.observe",
        "coverage":"local_quote_observed"}
    raw = as_json(evidence)
    db.execute("INSERT INTO g7_paired_decision_evidence VALUES (?,?,?,?,?)",
               (RUN,BAR,OBS,raw,sha(raw)))
    db.commit()
    return db,adaptive


def add_command(db, adaptive, *, target="demo", state="observed"):
    command={"id":CMD,"context":CTX,"brain":"system","kind":"open",
             "created_ms":OBS,"server":"Demo","login":17,"symbol":"XAUUSD",
             "currency":"USD","target":target,"signal":adaptive,
             "learning":{"trading_mode":"expert","timeframe":"M1","report_id":""}}
    result={"deal":OPEN,"position_id":POS}
    db.execute("INSERT INTO broker_commands VALUES (?,?,?,?,?,?,?,?)",
               (CMD,CTX,"system","open",OBS,state,as_json(command),as_json(result)))


def add_closed(db, *, late_admission=False, tamper_deal=False):
    deal_common={"symbol":"XAUUSD","type":0,"position_id":POS,"volume":.01,
                 "swap":0.0,"fee":0.0}
    opening=dict(deal_common,ticket=OPEN,entry=0,time_msc=241010,
                 profit=0.,commission=-.035)
    exit_=dict(deal_common,ticket=702,entry=1,time_msc=CLOSED,
               profit=10.,commission=-.035)
    db.execute("INSERT INTO broker_deals VALUES (?,?,?,?,?)",
               (CTX,OPEN,POS,"system",as_json(opening)))
    db.execute("INSERT INTO broker_deals VALUES (?,?,?,?,?)",
               (CTX,702,POS,"system",as_json(exit_)))
    gross,comm,net,risk=10.,-.07,9.93,5.
    label={"evidence_source":"mt5_demo_broker","context":CTX,"position_id":POS,
           "brain":"system","trade_owner":"system","trading_mode":"expert",
           "timeframe":"M1","report_id":"","executed_side":1,
           "open_command_id":CMD,"open_deal":OPEN,"closed_ms":CLOSED,
           "closed_volume":.01,"gross":gross,"commission":comm,"swap":0.0,
           "fee":0.0,"net":net,"net_per_lot":net/.01,
           "risk_money":risk,"r_multiple":net/risk,"reward":math.tanh(net/risk)}
    source=as_json([CTX,"adaptive-member-v1","expert","system","M1",""])
    raw=as_json(label)
    db.execute("INSERT INTO coordinator_broker_samples VALUES (?,?,?,?,?,?)",
               (source,POS,CTX,"system",CLOSED,raw))
    db.execute("INSERT INTO coordinator_broker_admissions VALUES (?,?,?,?,?,?,?,?)",
               (source,POS,CTX,"system",CLOSED,299000 if late_admission else ADMITTED,
                hashlib.sha256(raw.encode()).hexdigest(),
                "g6g-runtime-same-transaction-v1"))
    if tamper_deal:
        exit_["profit"]=20.
        db.execute("UPDATE broker_deals SET payload=? WHERE context=? AND ticket=702",
                   (as_json(exit_),CTX))


class G7BookkeepingTests(unittest.TestCase):
    def test_no_market_data_does_not_fabricate_success(self):
        with sqlite3.connect(":memory:") as db:
            r=audit(db,now_ms=260000)
            self.assertEqual(r["status"],"not_started")
            self.assertIsNone(r["frozen_broker_pnl"])

    def test_valid_fully_reconciled_demo_adaptive_only(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            add_closed(db)
            r=audit(db,now_ms=260000)
            self.assertEqual(r["verified_closed_adaptive_positions"],1)
            self.assertAlmostEqual(r["booked_adaptive_net"],9.93)
            self.assertIsNone(r["frozen_broker_pnl"])
            self.assertIsNone(r["paired_broker_delta"])
            self.assertFalse(r["release_authorized"])
            self.assertEqual(r["status"],"inconclusive")

    def test_no_command_is_zero_execution_not_zero_profit(self):
        db,_=make_db()
        with db:
            r=audit(db,now_ms=260000)
            self.assertEqual(r["counts"]["no_exact_broker_open_intent"],1)
            self.assertIsNone(r["booked_adaptive_net"])

    def test_open_command_without_full_close_remains_unverified(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            r=audit(db,now_ms=260000)
            self.assertEqual(r["verified_closed_adaptive_positions"],0)
            self.assertIn("missing_accepted_closed_broker_label",r["unverified_by_reason"])

    def test_changed_broker_deal_cost_excludes_close(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            add_closed(db,tamper_deal=True)
            r=audit(db,now_ms=260000)
            self.assertEqual(r["verified_closed_adaptive_positions"],0)
            self.assertIn("broker_receipt_disagrees_with_mt5_deal_history",r["unverified_by_reason"])

    def test_late_admission_excludes_label(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            add_closed(db,late_admission=True)
            r=audit(db,now_ms=260000)
            self.assertIn("changed_or_late_admitted_broker_label",r["unverified_by_reason"])

    def test_real_target_is_not_demo(self):
        db,a=make_db()
        with db:
            add_command(db,a,target="live")
            add_closed(db)
            with self.assertRaisesRegex(ValueError,"not_verified_demo_identity"):
                audit(db,now_ms=260000)

    def test_command_only_similar_direction_cannot_match(self):
        db,a=make_db()
        with db:
            mismatched=dict(a,key="different_strategy")
            add_command(db,mismatched)
            add_closed(db)
            r=audit(db,now_ms=260000)
            self.assertEqual(r["counts"]["no_exact_broker_open_intent"],1)
            self.assertIsNone(r["booked_adaptive_net"])

    def test_tampered_g7_decision_rejected_before_any_broker_link(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            add_closed(db)
            db.execute("UPDATE frozen_coordinator_decisions SET adaptive='{}'")
            with self.assertRaisesRegex(ValueError,"source/provenance"):
                audit(db,now_ms=260000)

    def test_uncovered_volume_excludes_unsettled_position(self):
        db,a=make_db()
        with db:
            add_command(db,a)
            add_closed(db)
            raw=db.execute("SELECT payload FROM broker_deals WHERE ticket=702").fetchone()[0]
            deal=json.loads(raw)
            deal["volume"]=.005
            db.execute("UPDATE broker_deals SET payload=? WHERE ticket=702",(as_json(deal),))
            r=audit(db,now_ms=260000)
            self.assertEqual(r["verified_closed_adaptive_positions"],0)
            self.assertIn("broker_entry_exit_not_fully_reconciled",r["unverified_by_reason"])


if __name__=="__main__":
    unittest.main()
