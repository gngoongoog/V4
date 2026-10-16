"""Import-only runtime check. Does not connect to MT5 or Telegram."""
import json
from pathlib import Path
import platform
import sys

def main():
    report = {"python": platform.python_version(), "architecture": platform.machine(), "isolated": bool(sys.flags.isolated), "ok": False}
    try:
        import numpy
        import MetaTrader5
        import fast_io
        import correlation_feed
        import multiprocessing
        report.update(numpy=numpy.__version__, mt5=MetaTrader5.__version__, ok=True, research_modules=True)
    except Exception as error:
        report["error"] = type(error).__name__ + ": " + str(error)
    text = json.dumps(report, ensure_ascii=False, indent=2)
    if len(sys.argv) > 1:
        Path(sys.argv[1]).write_text(text, encoding="utf-8")
    print(text)
    return 0 if report["ok"] else 1

if __name__ == "__main__":
    raise SystemExit(main())
