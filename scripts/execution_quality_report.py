"""Read-only CLI: inspect measured gateway dispatch latency after a Demo session."""
from pathlib import Path
import argparse
import json
import sqlite3
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT / 'gateway'))
from institutional_controls import quality_report


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--db',type=Path,required=True,help='Path to broker-journal.sqlite')
    parser.add_argument('--limit',type=int,default=1000)
    args=parser.parse_args()
    if not args.db.is_file():
        parser.error('Journal database does not exist')
    # SQLite read-only: never mutates broker send journal.
    db=sqlite3.connect(f'file:{args.db.resolve().as_posix()}?mode=ro',uri=True,timeout=1)
    try: print(json.dumps(quality_report(db,args.limit),ensure_ascii=False,indent=2))
    finally:db.close()


if __name__=='__main__':main()
