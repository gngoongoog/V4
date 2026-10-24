"""Read-only strategy comparison; never claims or authorizes live execution."""
from pathlib import Path
import argparse
import json
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'gateway'))
from strategy_lab import study

def main():
    p=argparse.ArgumentParser()
    p.add_argument('--deals',required=True,type=Path,help='JSON array of confirmed MT5 Demo close deals')
    p.add_argument('--output',type=Path,help='Optional JSON report path')
    args=p.parse_args()
    if not args.deals.is_file() or args.deals.stat().st_size>50_000_000:
        p.error('Evidence file missing or larger than 50MB')
    data=json.loads(args.deals.read_text('utf8'))
    result=study(data)
    rendered=json.dumps(result,indent=2,ensure_ascii=False)
    if args.output:
        if args.output.resolve()==args.deals.resolve():p.error('Never overwrite evidence input')
        args.output.write_text(rendered+'\n','utf8')
    print(rendered)

if __name__=='__main__':main()
