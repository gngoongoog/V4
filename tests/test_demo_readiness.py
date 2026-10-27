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

def bound_execution(**changes):
    item={
        "settings":{"target":"demo","login":474654523,
                    "server":"Demo Server","currency":"USD"},
        "account_matches":True,
        "context":json.dumps(["Demo Server",474654523,"XAUUSD","USD"])
    }
    item.update(changes)
    return item

class Tests(unittest.TestCase):
    def test_fresh_demo_without_broker_orders_does_not_pass_g2(self):
        r=readiness.evaluate(status(),CONFIG,NOW,execution=bound_execution())
        self.assertEqual(r["result"],"CURRENT_DEMO_ACCOUNT_AND_QUOTE_OBSERVED")
        self.assertEqual(r["quote_status"],"FRESH")
        self.assertEqual(r["execution_binding_status"],"BOUND_DEMO")
        self.assertFalse(r["preflight_ready_to_trade"])
        self.assertEqual(r["account_snapshot_status"],"VERIFIED_DEMO")
        self.assertTrue(r["used_margin_reported"])
        self.assertEqual(r["broker_lifecycle_acceptance"],"NOT_TESTED")
        self.assertEqual(r["ea_metaeditor_acceptance"],"NOT_TESTED")
        self.assertFalse(r["execution_enabled"])
        self.assertNotIn("474654523",json.dumps(r))
        self.assertNotIn("Demo Server",json.dumps(r))

    def test_closed_market_stale_quote_keeps_demo_account_verified(self):
        r=readiness.evaluate(status(tick_age_ms=50_000),CONFIG,NOW,execution=bound_execution())
        self.assertEqual(r["account_snapshot_status"],"VERIFIED_DEMO")
        self.assertEqual(r["result"],"QUOTE_STALE_OR_SESSION_INACTIVE")
        self.assertEqual(r["quote_status"],"NOT_TRUSTED")
        self.assertEqual(r["broker_lifecycle_acceptance"],"NOT_TESTED")

    def test_mismatched_symbol_and_absent_selected_symbol(self):
        r=readiness.evaluate(status(symbol="XAUUSDr"),CONFIG,NOW,execution=bound_execution())
        self.assertEqual(r["result"],"CONFIGURED_SYMBOL_MISMATCH")
        r=readiness.evaluate(status(error="Symbol XAUUSD is unavailable; choose its exact broker name in Settings."),CONFIG,NOW,execution=bound_execution())
        self.assertEqual(r["result"],"SELECTED_SYMBOL_UNAVAILABLE")
        self.assertTrue(r["symbol_unavailable_at_mt5"])
        r=readiness.evaluate(status(error="Symbol XAUUSDr is unavailable; choose its exact broker name in Settings."),CONFIG,NOW,execution=bound_execution())
        self.assertEqual(r["result"],"CURRENT_DEMO_ACCOUNT_AND_QUOTE_OBSERVED",
                         "Old error for another symbol cannot invalidate live exact symbol")

    def test_unknown_quote_time_or_corrupt_price_never_certified(self):
        t=status(source_quote_time_ms=None,quote_time_ms="bad")
        self.assertEqual(readiness.evaluate(t,CONFIG,NOW,execution=bound_execution())["result"],"QUOTE_TIME_UNVERIFIED")
        t=status(ask=2000,bid=2600)
        self.assertEqual(readiness.evaluate(t,CONFIG,NOW,execution=bound_execution())["result"],"NO_VALID_BID_ASK")
        t=status(margin=False)
        self.assertFalse(readiness.evaluate(t,CONFIG,NOW,execution=bound_execution())["used_margin_reported"])

    def test_real_account_and_gateway_offline_refused(self):
        self.assertEqual(readiness.evaluate(status(demo=False,account_trade_mode=2),CONFIG,NOW,execution=bound_execution())["result"],
                         "DEMO_ACCOUNT_NOT_VERIFIED")
        s=status();s["gateway_fresh"]=False
        self.assertEqual(readiness.evaluate(s,CONFIG,NOW,execution=bound_execution())["result"],"BROKER_DISCONNECTED")


    def test_connected_demo_does_not_prove_execution_account_binding(self):
        base=status()
        self.assertEqual(readiness.evaluate(base,CONFIG,NOW)["result"],
                         "EXECUTION_BINDING_UNAVAILABLE")
        self.assertEqual(readiness.evaluate(base,CONFIG,NOW,execution={})["result"],
                         "EXECUTION_BINDING_UNAVAILABLE")
        live=bound_execution(settings={**bound_execution()["settings"],"target":"live"})
        r=readiness.evaluate(base,CONFIG,NOW,execution=live)
        self.assertEqual(r["result"],"EXECUTION_TARGET_NOT_DEMO")
        self.assertEqual(r["account_snapshot_status"],"VERIFIED_DEMO")
        self.assertEqual(r["quote_status"],"FRESH")
        self.assertFalse(r["preflight_ready_to_trade"])
        self.assertEqual(readiness.evaluate(base,CONFIG,NOW,
            execution=bound_execution(account_matches=False))["result"],
            "DEMO_EXECUTION_BINDING_MISMATCH")
        for wrong in (
            bound_execution(context=json.dumps(["Other",474654523,"XAUUSD","USD"])),
            bound_execution(context=json.dumps(["Demo Server",474654523,"XAUUSDr","USD"])),
            bound_execution(context="not-json"),
            bound_execution(settings={**bound_execution()["settings"],"login":99}),
        ):
            with self.subTest(wrong=wrong):
                report=readiness.evaluate(base,CONFIG,NOW,execution=wrong)
                self.assertEqual(report["result"],"DEMO_EXECUTION_BINDING_MISMATCH")
                serialized=json.dumps(report)
                self.assertNotIn("474654523",serialized)
                self.assertNotIn("Demo Server",serialized)

    def test_demo_account_switch_cannot_pass_bound_preflight(self):
        r=readiness.evaluate(status(login=99),CONFIG,NOW,
            execution=bound_execution())
        self.assertEqual(r["result"],"DEMO_EXECUTION_BINDING_MISMATCH")
        self.assertEqual(r["execution_binding_status"],"MISMATCH")
        self.assertEqual(r["broker_lifecycle_acceptance"],"NOT_TESTED")

    def test_privacy_html_escape_and_power_shell_wiring(self):
        with tempfile.TemporaryDirectory() as tmp:
            r=readiness.evaluate(status(symbol="<script>"),{"symbol":"<script>"},NOW,execution=bound_execution())
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
        self.assertIn('if ($PreflightOnly) {',ps)
        self.assertIn('if (-not $PreflightOnly -and -not $PSBoundParameters.ContainsKey("Source")) {',ps)
        self.assertIn('if len(sys.argv) > 1 and sys.argv[1] == "--demo-readiness":',cli)
        self.assertIn('execution = api.get("/v1/execution")',cli)
        self.assertIn('evaluate(status, config, execution=execution)',cli)
        import ast
        tree=ast.parse(FILE.read_text("utf-8"))
        calls=[node.func for node in ast.walk(tree) if isinstance(node,ast.Call)]
        forbidden={"order_send","initialize","order_check","positions_close"}
        self.assertFalse(any(
            (isinstance(call,ast.Name) and call.id in forbidden)
            or (isinstance(call,ast.Attribute) and call.attr in forbidden)
            for call in calls
        ),"Read-only preflight may not initialize MT5 or submit broker orders")

if __name__=="__main__":
    unittest.main()
