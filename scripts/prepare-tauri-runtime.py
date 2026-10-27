"""Populate installer runtime: verified, pinned Windows Python and MetaTrader5 modules.
Always verify SHA-256 before unpacking, and reject zip traversal/symlinks.
Does not compile EA (MetaEditor/terminal action required on actual Windows MT5).
"""
from __future__ import annotations
import hashlib
import json
import shutil
from pathlib import Path
import sys
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'scripts'))
from importlib.machinery import SourceFileLoader
pkg=SourceFileLoader('braintrade_pkg',str(ROOT/'scripts'/'package-portable.py')).load_module()
STAGE=ROOT/'apps'/'tauri'

def main():
  cache=ROOT/'.package-cache';cache.mkdir(parents=True,exist_ok=True)
  python=STAGE/'runtime'/'python'
  for asset in pkg.ASSETS:
    archive=pkg.download(asset,cache)
    destination=STAGE/asset['destination']
    pkg.extract_checked(archive,destination)
  (python/'python313._pth').write_text('python313.zip\n.\nLib/site-packages\n../gateway\nimport site\n',encoding='ascii')
  gateway=STAGE/'runtime'/'gateway';gateway.mkdir(parents=True,exist_ok=True)
  for f in pkg.GATEWAY_FILES:shutil.copy2(ROOT/'gateway'/f,gateway/f)
  mt5=STAGE/'mt5'/'Experts';mt5.mkdir(parents=True,exist_ok=True)
  shutil.copy2(ROOT/'mt5'/pkg.NATIVE_EA,mt5/pkg.NATIVE_EA)
  p=STAGE/'runtime'/'validation.json'
  p.write_text(json.dumps({'source':'pinned SHA256 verified Windows embedded assets','assets':pkg.ASSETS,'build_role':'Windows Tauri installer runtime'},indent=2),encoding='utf-8')
  print('Prepared SHA256-verified runtime:',python,'and',gateway)
if __name__=='__main__':main()
