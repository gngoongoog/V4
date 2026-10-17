from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
REPLAY = ROOT / "crates" / "runtime" / "src" / "news" / "replay.rs"
NEWS = ROOT / "crates" / "runtime" / "src" / "news.rs"


class NewsReplayContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.replay = REPLAY.read_text(encoding="utf-8")
        cls.news = NEWS.read_text(encoding="utf-8")

    def test_replay_is_research_only_and_has_no_execution_route(self):
        self.assertIn('"research_only":true', self.replay)
        self.assertIn("No BUY/SELL inference and no broker execution effect", self.replay)
        self.assertNotIn("order_send", self.replay)
        self.assertNotIn("/v1/execution", self.replay)
        self.assertNotIn("execution::", self.replay)

    def test_first_observed_time_controls_availability(self):
        self.assertIn("first_observed_ms", self.replay)
        self.assertIn("next_eligible_bar", self.replay)
        self.assertIn("Publisher timestamps never grant earlier availability", self.replay)
        self.assertIn("m1_gap_no_forward_fill", self.replay)

    def test_news_service_persists_observations_and_routes_replay(self):
        self.assertIn("replay::init(db)?", self.news)
        self.assertIn("replay::observe(db, &source.id, source.last_success_ms, &source.items)?", self.news)
        self.assertIn('path == "/v1/news/replay"', self.news)
        self.assertIn("replay::route(method, path, body, db, now)", self.news)


if __name__ == "__main__":
    unittest.main()
