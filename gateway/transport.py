"""Persistent, single-threaded loopback HTTP client for Core hot-path requests.

This never automatically retries a request. In particular a lost reply to a
POST is ambiguous and MUST NOT be resent by a transport helper.
"""
from __future__ import annotations
import http.client
import json
import threading

class LoopbackCore:
    def __init__(self, bearer_token: str, host='127.0.0.1', port=18741, timeout=4.0):
        if host != '127.0.0.1' or not isinstance(bearer_token,str) or not bearer_token:
            raise ValueError('Loopback Core transport requires bearer auth and loopback')
        self.token=bearer_token
        self.host=host
        self.port=port
        self.timeout=timeout
        self.conn=None
        self.owner=threading.get_ident()
        self.reconnects=0
        self.requests=0

    def close(self):
        if self.conn is not None:
            try:self.conn.close()
            finally:self.conn=None

    def _request(self,path,body=None,method=None,expect_json=True):
        if threading.get_ident()!=self.owner:
            raise RuntimeError('Core HTTP session is bound to gateway main thread')
        if not isinstance(path,str) or not path.startswith('/v1/') or '://' in path:
            raise ValueError('Invalid Core API path')
        if self.conn is None:
            self.conn=http.client.HTTPConnection(self.host,self.port,timeout=self.timeout)
            self.reconnects+=1
        payload=json.dumps(body,allow_nan=False,separators=(',',':')).encode('utf-8') if body is not None else None
        verb=method or ('POST' if payload is not None else 'GET')
        if verb not in ('GET','POST','PUT'):
            raise ValueError('Unsupported Core API method')
        headers={'Authorization':'Bearer '+self.token,'Content-Type':'application/json',
                 'Accept':'application/json'}
        try:
            self.conn.request(verb,path,body=payload,headers=headers)
            response=self.conn.getresponse()
            # Bounded JSON/Core error: never read an unbounded response from
            # unexpected local listener. Normal snapshot sizes are < 1MB.
            data=response.read(4_194_305)
            if len(data)>4_194_304:
                raise ValueError('Core response exceeded 4MB')
            if response.status < 200 or response.status >= 300:
                raise OSError(f'Core returned HTTP {response.status} for {path}')
            self.requests+=1
            return json.loads(data) if expect_json else None
        except (OSError, http.client.HTTPException, ValueError):
            # No retries: a POST may already have committed before failure.
            self.close()
            raise

    def api(self,path,payload=None,method=None):
        return self._request(path,payload,method)

    def publish(self,snapshot):
        return self._request('/v1/gateway/snapshot',snapshot,method='POST',expect_json=False)
