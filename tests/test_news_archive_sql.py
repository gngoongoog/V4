"""Exercise the actual archive SELECT against SQLite; Rust execution remains a separate build gate."""
import json
from pathlib import Path
import re
import sqlite3
import unittest

class ArchiveSqlTests(unittest.TestCase):
    def test_replay_selects_only_latest_revision_available_at_cutoff(self):
        source=(Path(__file__).resolve().parents[1]/'crates/runtime/src/news_archive.rs').read_text()
        schema=re.search(r'db.execute_batch\("([^"]+)"\)',source).group(1)
        query=re.search(r'let mut q=db.prepare\("([^"]+)"\)',source).group(1)
        db=sqlite3.connect(':memory:');db.executescript(schema)
        try:
            for at,title in [(3000,'original'),(5000,'revision'),(5000,'same-time later revision')]:
                db.execute('INSERT INTO news_observations(source,url,observed_ms,payload) VALUES(?,?,?,?)',('fed','https://example',at,json.dumps(dict(title=title,published_ms=1000))))
            self.assertEqual(db.execute(query,(2000,0)).fetchall(),[])
            past=db.execute(query,(4000,0)).fetchall();self.assertEqual(len(past),1);self.assertEqual(json.loads(past[0][2])['title'],'original')
            current=db.execute(query,(6000,0)).fetchall();self.assertEqual(len(current),1);self.assertEqual(json.loads(current[0][2])['title'],'same-time later revision')
            self.assertEqual(db.execute(query,(6000,5500)).fetchall(),[])
        finally:db.close()

if __name__=='__main__':unittest.main()
