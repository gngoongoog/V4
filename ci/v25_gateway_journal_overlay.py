"""V25 P0 overlay on the exact V24 CI-restored source. Fails on unknown layout."""
from pathlib import Path

path=Path("gateway/execution.py")
original=path.read_text(encoding="utf-8")
old="""            journal_start=time.perf_counter()
            self.db.execute('INSERT INTO sends(id,request,result,created_ms) VALUES(?,?,NULL,?)',(cmd['id'],json.dumps(dict(cmd,broker_request=request),allow_nan=False),clock_ms()))
            self.db.commit()
            result['journal_ms']=(time.perf_counter()-journal_start)*1000
            result['request']=request
"""
new="""            journal_start=time.perf_counter()
            try:
                self.db.execute('INSERT INTO sends(id,request,result,created_ms) VALUES(?,?,NULL,?)',
                                (cmd['id'],json.dumps(dict(cmd,broker_request=request),allow_nan=False),clock_ms()))
                self.db.commit()
            except sqlite3.IntegrityError:
                # Another gateway worker already committed the same intent.
                # No second order_send is allowed.
                self.db.rollback()
                return self.execute(mt5,cmd,authorize)
            except sqlite3.OperationalError:
                # A locked/unwritable journal cannot guarantee idempotency.
                self.db.rollback()
                result.update(state='unknown',reason='Durable broker journal unavailable; order was not sent')
                result['total_ms']=(time.perf_counter()-started)*1000
                return result
            result['journal_ms']=(time.perf_counter()-journal_start)*1000
            result['request']=request
"""
if original.count(old)==1:
    path.write_text(original.replace(old,new),encoding="utf-8")
elif original.count("Durable broker journal unavailable; order was not sent")==1:
    print("Existing V25 journal guard already present")
else:
    raise SystemExit("Unrecognized gateway source; refusing blind overlay")
print("V25 broker journal regression protection staged")
