from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ChallengerPerformanceContractTests(unittest.TestCase):
    def test_policy_production_path_is_query_only(self):
        source = (ROOT / "crates/runtime/src/learning_performance.rs").read_text(encoding="utf-8")
        production = source.split("#[cfg(test)]", 1)[0]
        for forbidden in (
            ".execute(",
            ".execute_batch(",
            "unchecked_transaction",
            "INSERT INTO",
            "UPDATE learning_",
            "DELETE FROM learning_",
            "order_send(",
            "TRADE_ACTION_",
        ):
            self.assertNotIn(forbidden, production)
        self.assertIn('"promotion_eligible":false', production)
        self.assertIn('"promotion_enabled":false', production)
        self.assertIn('"shadow_effect":false', production)
        self.assertIn('"risk_effect":false', production)
        self.assertIn('"broker_effect":false', production)
        for verdict in ("insufficient_evidence", "review_candidate", "reject"):
            self.assertIn(verdict, production)

    def test_core_exposes_get_only_performance_route(self):
        source = (ROOT / "crates/runtime/src/lib.rs").read_text(encoding="utf-8")
        self.assertIn('method=="GET"&&path=="/v1/learning/performance"', source)
        self.assertNotIn('method=="POST"&&path=="/v1/learning/performance"', source)
        self.assertIn('"performance_review".into(),learning_performance::review(&self.db)?', source)

    def test_cli_opens_database_read_only(self):
        source = (ROOT / "crates/runtime/src/bin/challenger_review.rs").read_text(encoding="utf-8")
        self.assertIn("SQLITE_OPEN_READ_ONLY", source)
        self.assertNotIn("SQLITE_OPEN_READ_WRITE", source)
        self.assertNotIn("SQLITE_OPEN_CREATE", source)


if __name__ == "__main__":
    unittest.main()
