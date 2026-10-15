from __future__ import annotations

from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace as N
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "gateway"))
sys.path.insert(0, str(ROOT / "tests"))

from execution import BrokerExecutor
from test_broker_execution import Broker, command


class ExecutionRecoveryContractTests(unittest.TestCase):
    """Regression coverage for recovery paths that must never duplicate broker sends."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.path = Path(self.temp.name) / "broker-journal.sqlite"
        self.executor = BrokerExecutor(self.path)
        self.broker = Broker()
        self.cmd = command()

    def tearDown(self):
        self.executor.close()
        self.temp.cleanup()

    def execute(self):
        return self.executor.execute(self.broker, self.cmd, lambda _: True)

    def test_unknown_intent_is_account_scoped_and_recovers_without_resend(self):
        self.broker.raise_send = True
        first = self.execute()
        self.assertEqual(first["state"], "unknown")
        self.assertEqual(len(self.broker.sends), 1)

        # A different logged-in account must not reinterpret or replay the old intent.
        self.broker.raise_send = False
        self.broker.account.login = 99
        foreign = self.execute()
        self.assertEqual(foreign["state"], "unknown")
        self.assertEqual(len(self.broker.sends), 1)

        # Returning to the bound account may reconcile from history, but still may not resend.
        self.broker.account.login = 17
        self.broker.deals = [
            N(
                ticket=701,
                magic=self.cmd["magic"],
                symbol=self.cmd["symbol"],
                volume=0.1,
                comment="BT:" + self.cmd["id"][:20],
            )
        ]
        recovered = self.execute()
        self.assertEqual(recovered["state"], "filled")
        self.assertEqual(recovered["deal"], 701)
        self.assertEqual(recovered["request"]["volume"], 0.1)
        self.assertEqual(len(self.broker.sends), 1)

    def test_reconciliation_aggregates_split_deals_without_topping_up(self):
        self.broker.raise_send = True
        self.assertEqual(self.execute()["state"], "unknown")
        self.assertEqual(len(self.broker.sends), 1)

        self.broker.raise_send = False
        tag = "BT:" + self.cmd["id"][:20]
        self.broker.deals = [
            N(ticket=801, magic=self.cmd["magic"], symbol=self.cmd["symbol"], volume=0.04, comment=tag),
            N(ticket=802, magic=self.cmd["magic"], symbol=self.cmd["symbol"], volume=0.06, comment=tag),
        ]
        recovered = self.execute()
        self.assertEqual(recovered["state"], "filled")
        self.assertEqual(recovered["deals"], [801, 802])
        self.assertEqual(recovered["request"]["volume"], 0.1)
        self.assertEqual(len(self.broker.sends), 1)

    def test_same_journal_id_with_mutated_identity_fails_closed(self):
        self.broker.raise_send = True
        self.assertEqual(self.execute()["state"], "unknown")
        self.assertEqual(len(self.broker.sends), 1)

        # Simulate a restored/corrupted Core payload reusing the same ID for another identity.
        self.broker.raise_send = False
        self.cmd["server"] = "OtherServer"
        rejected = self.execute()
        self.assertEqual(rejected["state"], "unknown")
        self.assertIn("Intent identity differs", rejected["reason"])
        self.assertEqual(len(self.broker.sends), 1)


if __name__ == "__main__":
    unittest.main()
