"""P0/G2 safe Demo preflight: no order execution and honest acceptance gates."""
from __future__ import annotations
from pathlib import Path
import importlib.util
import json
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
FILE=ROOT/"gateway/demo_readiness.py"
spec=importlib.util.spec_from_file_location("v25_demo_readiness",FILE)
assert spec is not None and spec.loader is not None
readiness=importlib.util.module_from_spec(spec)
spec.loader.exec_module(readiness)

NOW=1800000000000

def status(**telemetry_overrides):
    telemetry={
        "connected":True,"demo":True,"account_trade_mode":0,
        "login":474654523,"server":"Demo Server","currency":"USD",
        "symbol":"XAUUSD","balance":250.0,"equity":250.0,
        "margin":0.0,"margin_free":250.0,"bid":2600.0,"ask":2600.1,
        "tick_age_ms":100,
        "quote_time_ms":NOW-100,"source_quote_time_ms":NOW-100,
    }
    telemetry.update(telemetry_overrides)
    return {"gateway_fresh":True,"last_received_ms":NOW-150,
            "requested_run":False,"execution_enabled":False,
            "telemetry":telemetry}
CONFIG={"symbol":"XAUUSD"}

class Tests(unittest.TestCase):
    def test_fresh_demo_without_broker_orders_does_not_pass_g2(self):
        r=readiness.evaluate(status(),CONFIG,NOW)
        self.assertEqual(r["result"],"CURRENT_DEMO_ACCOUNT_AND_QUOTE_OBSERVED")
        self.assertEqual(r["quote_status"],"FRESH")
        self.assertEqual(r["account_snapshot_status"],"VERIFIED_DEMO")
        self.assertTrue(r["used_margin_reported"])
        self.assertEqual(r["broker_lifecycle_acceptance"],"NOT_TESTED")
        self.assertEqual(r["ea_metaeditor_acceptance"],"NOT_TESTED")
        self.assertFalse(r["execution_enabled"])
        self.assertNotIn("474654523",json.dumps(r))
        self.assertNotIn("Demo Server",json.dumps(r))

    def test_closed_market_stale_quote_keeps_demo_account_verified(self):
        r=readiness.evaluate(status(tick_age_ms=50_000),CONFIG,NOW)
        self.assertEqual(r["account_snapshot_status"],"VERIFIED_DEMO")
        self.assertEqual(r["result"],"QUOTE_STALE_OR_SESSION_INACTIVE")
        self.assertEqual(r["quote_status"],"NOT_TRUSTED")
        self.assertEqual(r["broker_lifecycle_acceptance"],"NOT_TESTED")

    def test_mismatched_symbol_and_absent_selected_symbol(self):
        r=readiness.evaluate(status(symbol="XAUUSDr"),CONFIG,NOW)
        self.assertEqual(r["result"],"CONFIGURED_SYMBOL_MISMATCH")
        r=readiness.evaluate(status(error="Symbol XAUUSD is unavailable; choose its exact broker name in Settings."),CONFIG,NOW)
        self.assertEqual(r["result"],"SELECTED_SYMBOL_UNAVAILABLE")
        self.assertTrue(r["symbol_unavailable_at_mt5"])
        r=readiness.evaluate(status(error="Symbol XAUUSDr is unavailable; choose its exact broker name in Settings."),CONFIG,NOW)
        self.assertEqual(r["result"],"CURRENT_DEMO_ACCOUNT_AND_QUOTE_OBSERVED",
                         "Old error for another symbol cannot invalidate live exact symbol")

    def test_unknown_quote_time_or_corrupt_price_never_certified(self):
        t=status(source_quote_time_ms=None,quote_time_ms="bad")
        self.assertEqual(readiness.evaluate(t,CONFIG,NOW)["result"],"QUOTE_TIME_UNVERIFIED")
        t=status(ask=2000,bid=2600)
        self.assertEqual(readiness.evaluate(t,CONFIG,NOW)["result"],"NO_VALID_BID_ASK")
        t=status(margin=False)
        self.assertFalse(readiness.evaluate(t,CONFIG,NOW)["used_margin_reported"])

    def test_real_account_and_gateway_offline_refused(self):
        self.assertEqual(readiness.evaluate(status(demo=False,account_trade_mode=2),CONFIG,NOW)["result"],
                         "DEMO_ACCOUNT_NOT_VERIFIED")
        s=status();s["gateway_fresh"]=False
        self.assertEqual(readiness.evaluate(s,CONFIG,NOW)["result"],"BROKER_DISCONNECTED")

    def test_privacy_html_escape_and_power_shell_wiring(self):
        with tempfile.TemporaryDirectory() as tmp:
            r=readiness.evaluate(status(symbol="<script>"),{"symbol":"<script>"},NOW)
            j,h=readiness.save_report(r,Path(tmp))
            content=h.read_text("utf-8")
            self.assertIn("&lt;script&gt;",content)
            self.assertNotIn("<script>",content)
            report=json.loads(j.read_text("utf-8"))
            self.assertNotIn("login",json.dumps(report))
            self.assertEqual(report["broker_lifecycle_acceptance"],"NOT_TESTED")
        ps=(ROOT/"scripts/Run-Demo-Acceptance.ps1").read_text("utf-8")
        cli=(ROOT/"gateway/check_runtime.py").read_text("utf-8")
        self.assertIn("[switch]$PreflightOnly",ps)
        self.assertIn('"--demo-readiness"',ps)
        self.assertIn('if $', 'if $') # keep assertions independent of Powershell interpolation
        self.assertIn('if len(sys.argv) > 1 and sys.argv[1] == "--demo-readiness":',cli)
        self.assertNotIn("order_send(",readiness.FILE.read_text("utf-8"))

if __name__=="__main__":
    unittest.main()
