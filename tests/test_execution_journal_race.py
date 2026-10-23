"""P0-B broker journal race regressions (simulated MT5, no live orders)."""
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/"gateway"))
sys.path.insert(0,str(ROOT/"tests"))
from execution import BrokerExecutor
from test_broker_execution import Broker,command


class BrokerJournalRaceTests(unittest.TestCase):
    def test_busy_durable_journal_fails_closed_without_broker_send(self):
        with tempfile.TemporaryDirectory() as home:
            db=Path(home)/"broker-journal.sqlite"
            worker,holder=BrokerExecutor(db),BrokerExecutor(db)
            broker,cmd=Broker(),command()
            try:
                holder.db.execute("BEGIN IMMEDIATE")
                result=worker.execute(broker,cmd,lambda _: True)
                self.assertEqual(result["state"],"unknown",result)
                self.assertIn("journal unavailable",result["reason"])
                self.assertEqual(broker.sends,[])
            finally:
                holder.db.rollback()
                worker.close()
                holder.close()

    def test_late_durable_insert_conflict_never_crashes_or_resends(self):
        with tempfile.TemporaryDirectory() as home:
            db=Path(home)/"broker-journal.sqlite"
            first,second=BrokerExecutor(db),BrokerExecutor(db)
            broker,cmd=Broker(),command()
            raced=[]
            try:
                def concurrent_worker():
                    broker.on_check=None
                    raced.append(second.execute(broker,cmd,lambda _: True))
                broker.on_check=concurrent_worker
                result=first.execute(broker,cmd,lambda _: True)
                self.assertEqual(len(raced),1)
                self.assertEqual(raced[0]["state"],"filled")
                self.assertEqual(result["state"],"filled",result)
                self.assertEqual(len(broker.sends),1,"No duplicate order_send")
                row=first.db.execute("SELECT COUNT(*) FROM sends WHERE id=?",(cmd["id"],)).fetchone()
                self.assertEqual(row[0],1)
            finally:
                first.close()
                second.close()


if __name__=="__main__":
    unittest.main()
