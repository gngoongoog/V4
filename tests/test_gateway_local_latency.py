"""Core/MT5 local stage timing is bounded, monotonic, and diagnostic only."""
from pathlib import Path
from unittest import TestCase, main
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/"gateway"))
from mt5_gateway import GatewayLatencyWindow

class GatewayLatencyTests(TestCase):
    def test_bounded_percentiles_no_prices_or_tokens_and_reset_after_report(self):
        perf=GatewayLatencyWindow(limit=128,report_every=30.)
        self.assertIsNone(perf.report_due(perf.next_report-0.01))
        for n in range(1,201):
            perf.record("core_pulse_ms",float(n))
        perf.record("gateway_cycle_ms",4.5)
        perf.record("snapshot_ms",18.)
        perf.pulse_error()
        report=perf.report_due(perf.next_report)
        self.assertIn("BRAINTRADE_LOCAL_LATENCY",report)
        self.assertIn("core_pulse_ms:n=128",report)
        self.assertIn("max=200.00",report)
        self.assertIn("gateway_cycle_ms:n=1,p50=4.50",report)
        self.assertIn("snapshot_ms:n=1,p50=18.00",report)
        self.assertIn("core_pulse_errors=1",report)
        self.assertEqual(len(perf.samples["core_pulse_ms"]),0)
        self.assertEqual(perf.errors,0)
        self.assertNotIn("XAUUSD",report)

    def test_untrusted_stage_and_invalid_time_are_not_logged(self):
        perf=GatewayLatencyWindow(limit=5,report_every=1.)
        for bad in (float('nan'),float('inf'),-1.,1e6):
            perf.record("snapshot_ms",bad)
        self.assertEqual(list(perf.samples["snapshot_ms"]),[])
        with self.assertRaises(ValueError):
            perf.record("password",1.)
        self.assertIn("snapshot_ms=no_samples",perf.report_due(perf.next_report))

if __name__=="__main__":main()
