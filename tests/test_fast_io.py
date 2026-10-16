"""Fast-path behavior with local HTTP and fake MT5 only; never contacts a broker."""
from datetime import datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import sys
import threading
import unittest
from unittest.mock import Mock, patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1]/'gateway'))
from fast_io import CoreClient, CachedMetadata, LoopMetrics, ReadWorker, read_job
from mt5_gateway import collect, day_range, MarketCache
from test_gateway import FakeMT5
from test_broker_execution import Broker, command
from execution import BrokerExecutor
import tempfile


class FastIoTests(unittest.TestCase):
    def test_persistent_http_uses_one_connection_and_never_replays_failed_post(self):
        requests=[]; peers=[]
        class Handler(BaseHTTPRequestHandler):
            protocol_version='HTTP/1.1'
            def do_POST(self):
                requests.append(self.path);peers.append(self.client_address)
                self.rfile.read(int(self.headers.get('Content-Length',0)))
                if self.path=='/lost':
                    self.close_connection=True
                    return
                body=b'{"ok":true}'
                self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
            def log_message(self,*args):pass
        server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
        thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        client=CoreClient('test-token',server.server_port)
        try:
            self.assertTrue(client('/first',{})['ok']);self.assertTrue(client('/second',{})['ok'])
            self.assertEqual(peers[0],peers[1])
            with self.assertRaises(OSError):client('/lost',{})
            self.assertEqual(requests.count('/lost'),1)
            self.assertIsNone(client.connection)
            self.assertTrue(client('/next',{})['ok'])
        finally:
            client.close();server.shutdown();server.server_close();thread.join(timeout=2)

    def test_deadline_sleep_subtracts_processing_and_bounds_metrics_memory(self):
        m=LoopMetrics()
        with patch('fast_io.time.perf_counter',return_value=10.007):
            self.assertAlmostEqual(m.finish(10.,1.,2.,3.,20),.013)
        with patch('fast_io.time.perf_counter',return_value=10.030):
            self.assertEqual(m.finish(10.,1.,2.,3.,20),.001)
        with patch('fast_io.time.perf_counter',return_value=10.010):
            for _ in range(300):m.finish(10.,0.,0.,0.,20)
        self.assertEqual(m.last['samples'],256)
        self.assertAlmostEqual(m.last['p95_ms'],10.)

    def test_cached_snapshot_does_not_read_history_or_cross_account_day(self):
        mt=FakeMT5();now=datetime.now(timezone.utc);day=day_range(now)[2]
        cache=CachedMetadata();key=(123,'Test','XAUUSD','USD')
        cache.update(dict(identity=key,market={'captured_ms':1},net=7.,count=2,day=day))
        mt.history_deals_get=Mock(side_effect=AssertionError('History entered hot path'))
        snap=collect(mt,'XAUUSD',now,cache,None,cache)
        self.assertEqual(snap['today_net'],7.);self.assertEqual(snap['market'],{'captured_ms':1})
        mt.account.login=124
        snap=collect(mt,'XAUUSD',now,cache,None,cache)
        self.assertIsNone(snap['today_net']);self.assertIsNone(snap['market'])
        mt.account.login=123
        self.assertIsNone(collect(mt,'XAUUSD',now+timedelta(days=1),cache,None,cache)['today_net'])
        cache.received-=11
        self.assertIsNone(collect(mt,'XAUUSD',now,cache,None,cache)['today_net'])

    def test_market_refreshes_at_closed_minute_boundary(self):
        now=datetime(2026,10,16,10,0,59,tzinfo=timezone.utc);cache=MarketCache()
        with patch('mt5_gateway.collect_market',return_value={}) as read, patch('mt5_gateway.time.monotonic',return_value=10.):
            cache.read(None,(1,'A','X'),now)
            cache.read(None,(1,'A','X'),now+timedelta(seconds=1))
        self.assertEqual(read.call_count,2)

    def test_background_account_switch_discards_result(self):
        mt=FakeMT5()
        def switch(*args):mt.account.login=456;return []
        mt.history_deals_get=switch
        cache=Mock();cache.read.return_value={}
        with self.assertRaisesRegex(ValueError,'during background read'):
            read_job(mt,dict(kind='metadata',identity=(123,'Test','X','USD')),cache)

    def test_stalled_read_worker_is_terminated_without_receiving(self):
        worker=ReadWorker('');worker.pending={};worker.started=0.
        worker.pipe=Mock();worker.pipe.poll.return_value=False;pipe=worker.pipe
        worker.process=Mock();worker.process.is_alive.return_value=True;process=worker.process
        with patch('fast_io.time.monotonic',return_value=16.):self.assertIsNone(worker.poll())
        process.terminate.assert_called_once();process.join.assert_called_once_with(timeout=.2)
        pipe.recv.assert_not_called();self.assertFalse(worker.busy)

    def test_preloaded_cycle_skips_work_request_and_history(self):
        with tempfile.TemporaryDirectory() as directory:
            executor=BrokerExecutor(Path(directory)/'journal.sqlite');mt=Broker();api=Mock()
            mt.history_deals_get=Mock(side_effect=AssertionError('History entered hot path'))
            try:
                result=executor.cycle(mt,api,work=dict(command=None,history_from_ms=1,account={'login':17},poll_ms=20),include_history=False)
                self.assertEqual(result,.02);api.assert_not_called();mt.history_deals_get.assert_not_called()
            finally:executor.close()

    def test_fast_send_bypasses_precheck_but_preserves_authorization_identity_and_journal(self):
        with tempfile.TemporaryDirectory() as directory:
            executor=BrokerExecutor(Path(directory)/'journal.sqlite');mt=Broker();cmd=command();cmd['fast_path']=True
            mt.order_check=Mock(side_effect=AssertionError('Precheck should be bypassed'))
            try:
                result=executor.execute(mt,cmd,lambda _:False)
                self.assertEqual(result['state'],'cancelled');self.assertFalse(mt.sends)
                def switch(_):mt.account.login=88;return True
                self.assertEqual(executor.execute(mt,cmd,switch)['state'],'rejected');self.assertFalse(mt.sends)
                mt.account.login=17;mt.raise_send=True
                self.assertEqual(executor.execute(mt,cmd,lambda _:True)['state'],'unknown')
                mt.raise_send=False
                self.assertEqual(executor.execute(mt,cmd,lambda _:True)['state'],'unknown')
                self.assertEqual(len(mt.sends),1);mt.order_check.assert_not_called()
            finally:executor.close()

if __name__=='__main__':unittest.main()
