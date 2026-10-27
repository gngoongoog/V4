from __future__ import annotations
import json
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from pathlib import Path
import socket
import sys
import threading
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'gateway'))
from transport import LoopbackCore

class Service(BaseHTTPRequestHandler):
    protocol_version='HTTP/1.1'
    accepted=0
    calls=[]
    def setup(self):
        super().setup()
        type(self).accepted+=1
    def _serve(self):
        length=int(self.headers.get('Content-Length','0'))
        payload=self.rfile.read(length)
        type(self).calls.append((self.command,self.path,self.headers.get('Authorization'),payload))
        if self.path=='/v1/drop':
            self.connection.shutdown(socket.SHUT_RDWR)
            self.connection.close()
            return
        data=json.dumps({'ok':True,'path':self.path}).encode()
        self.send_response(200)
        self.send_header('Content-Type','application/json')
        self.send_header('Content-Length',str(len(data)))
        self.end_headers()
        self.wfile.write(data)
    do_GET=do_POST=do_PUT=_serve
    def log_message(self,*_):pass

class LoopbackTransportTests(unittest.TestCase):
    def setUp(self):
        Service.accepted=0;Service.calls=[]
        self.server=ThreadingHTTPServer(('127.0.0.1',0),Service)
        self.thread=threading.Thread(target=self.server.serve_forever,daemon=True)
        self.thread.start()
        self.client=LoopbackCore('local-secret',port=self.server.server_port,timeout=1)
    def tearDown(self):
        self.client.close()
        self.server.shutdown();self.server.server_close();self.thread.join(timeout=2)
    def test_keeps_one_tcp_connection_for_hot_path(self):
        for _ in range(8):
            self.assertTrue(self.client.api('/v1/status')['ok'])
        self.assertEqual(self.client.requests,8)
        self.assertEqual(self.client.reconnects,1)
        self.assertEqual(Service.accepted,1)
        self.assertEqual(len(Service.calls),8)
    def test_put_and_snapshot_json_auth_but_never_exposes_token_in_result(self):
        self.assertTrue(self.client.api('/v1/execution/pulse',{'bid':123.4},method='PUT')['ok'])
        self.client.publish({'connected':True})
        self.assertEqual(Service.calls[0][0],'PUT')
        self.assertEqual(Service.calls[0][2],'Bearer local-secret')
        self.assertEqual(json.loads(Service.calls[1][3]),{'connected':True})
    def test_disconnected_post_is_not_replayed_implicitly(self):
        with self.assertRaises((OSError,ConnectionError)):
            self.client.api('/v1/drop',{'order':'ambiguous'})
        self.assertEqual(len(Service.calls),1)
        self.assertEqual(self.client.requests,0)
        self.assertIsNone(self.client.conn)
        self.assertTrue(self.client.api('/v1/status')['ok'])
        self.assertEqual(len(Service.calls),2)
    def test_rejects_non_loopback_and_bad_path(self):
        with self.assertRaises(ValueError):LoopbackCore('key',host='0.0.0.0')
        with self.assertRaises(ValueError):self.client.api('http://evil.example/x')

if __name__=='__main__':unittest.main()
