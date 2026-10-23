"""UI-13 regression checks: exact reference layout and safe Rust Core integration."""
import re
import unittest
from pathlib import Path
from html.parser import HTMLParser

ROOT=Path(__file__).resolve().parents[1]
HTML=(ROOT/'ui/index.html').read_text(encoding='utf-8')
CSS=(ROOT/'ui/styles.css').read_text(encoding='utf-8')
JS=(ROOT/'ui/app.js').read_text(encoding='utf-8')
CS=(ROOT/'WindowsShell/Program.cs').read_text(encoding='utf-8')

class IDs(HTMLParser):
    def __init__(self): super().__init__(); self.ids=[]
    def handle_starttag(self,tag,attrs):
        props=dict(attrs)
        if 'id' in props:self.ids.append(props['id'])

class NextUIContract(unittest.TestCase):
    def test_unique_dom_ids_and_reference_panels(self):
        parsed=IDs();parsed.feed(HTML)
        self.assertEqual(len(parsed.ids),len(set(parsed.ids)))
        for id_ in ('dashboard','sidebarNav','candles','previewBanner','riskBars','equityChart','tradeDonut','brainsInline','positionsRows','journalRows','startBtn','stopBtn','modalLayer','topEquity','topBalance','topMargin','topFree','topDaily'):
            self.assertIn(id_,parsed.ids)
        self.assertEqual(len(re.findall(r'<article class="kpi ',HTML)),6)
        self.assertEqual(len(re.findall(r'class="panel ',HTML)),9)
    def test_no_invented_live_money_or_performance(self):
        self.assertIn("preview:!native",JS)
        self.assertIn("if(state.preview)return PREVIEW_CANDLES",JS)
        self.assertIn("const MOCK=Object.freeze",JS)
        self.assertIn("if(!data.gateway_fresh || !data.telemetry?.connected)",JS)
        self.assertIn("لا توجد بيانات كافية",HTML)
        self.assertIn("بانتظار سجل الصفقات المغلقة",JS)
    def test_native_core_auth_and_execution_confirmation(self):
        self.assertIn('DataProtectionScope.CurrentUser',CS)
        self.assertIn('AuthenticationHeaderValue("Bearer", key)',CS)
        self.assertIn('CoreWebView2HostResourceAccessKind.DenyCors',CS)
        self.assertIn('if (!e.Source.StartsWith(TrustedOrigin',CS)
        self.assertIn('path == "/v1/start" || path == "/v1/stop"',CS)
        self.assertIn('confirm_live_login',CS)
        self.assertIn("state.preview||!state.connected",JS)
        self.assertIn('!state.status?.requested_run',JS)
    def test_no_remote_code_execution_or_fake_broker(self):
        self.assertIn("default-src 'none'",HTML)
        self.assertIn("script-src 'self'",HTML)
        self.assertIn("connect-src 'none'",HTML)
        self.assertNotIn('https://cdn.',HTML+CSS+JS)
        self.assertNotIn('eval(',JS)
        self.assertIn('لا يتم رسم أسعار افتراضية',HTML)
    def test_dynamic_chart_and_real_positions(self):
        for marker in ("market[tf]", "t.positions", "step=plotW/last.length", "chartHover", "window.chrome.webview.postMessage", "state.ma", "selected", "setTheme"):
            self.assertIn(marker,JS)

if __name__=='__main__':unittest.main()