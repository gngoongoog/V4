import sys
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'gateway'))
from mt5_gateway import DomCache

class Book:
    def __init__(self):
        self.attempts=0
        self.released=[]
        self.rows=[]
        self.accept=True
    def market_book_add(self,symbol):
        self.attempts+=1
        return self.accept
    def market_book_release(self,symbol): self.released.append(symbol)
    def market_book_get(self,symbol): return self.rows

def level(kind,price,volume=1):
    return SimpleNamespace(type=kind,price=price,volume=volume,volume_real=0)

class DomTests(unittest.TestCase):
    def test_failed_subscription_retries_after_backoff(self):
        mt5=Book();cache=DomCache();key=('broker',1,'XAUUSD');mt5.accept=False
        with patch('mt5_gateway.time.monotonic',return_value=10):
            self.assertEqual(cache.read(mt5,key),(False,[]))
            cache.read(mt5,key)
        self.assertEqual(mt5.attempts,1)
        mt5.accept=True;mt5.rows=[level(2,100),level(1,101)]
        with patch('mt5_gateway.time.monotonic',return_value=41):
            self.assertTrue(cache.read(mt5,key)[0])
        self.assertEqual(mt5.attempts,2)
    def test_large_book_keeps_both_sides_and_combines_price_levels(self):
        mt5=Book();cache=DomCache()
        mt5.rows=[level(2,100-i*.01) for i in range(110)]+[level(2,100,3),level(1,101),level(4,99,999)]
        available,rows=cache.read(mt5,('broker',1,'XAUUSD'))
        self.assertTrue(available);self.assertEqual(len(rows),51)
        self.assertEqual(rows[0]['volume'],4)
        self.assertEqual(rows[-1]['side'],'sell')
        self.assertFalse(any(row['volume']==999 for row in rows))
    def test_account_switch_releases_old_subscription(self):
        mt5=Book();cache=DomCache();mt5.rows=[level(2,100),level(1,101)]
        cache.read(mt5,('broker',1,'XAUUSD'))
        cache.read(mt5,('broker',2,'XAUUSD'))
        self.assertEqual(mt5.released,['XAUUSD']);self.assertEqual(mt5.attempts,2)

if __name__=='__main__':unittest.main()
