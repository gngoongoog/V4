"""Reapply P0-B fill reconciliation rules to the exact restored V24 CI source.

Safe against repeated invocation; refuse source layouts we cannot verify.
V24's compiled logic and M15 rules remain unchanged.
"""
from pathlib import Path

p = Path("gateway/execution.py")
s = p.read_text(encoding="utf-8")
needle = "if result['state'] not in ('unknown', 'placed'):"
patched = "if result['state'] not in ('unknown', 'placed', 'partial'):"
acct_old = """        if account_identity(mt5.account_info()) != expected(cmd):
            return result
"""
acct_new = """        # A journal is scoped to both broker identity and Demo/REAL mode.
        # A matching login tuple alone must never authorize cross-mode recovery.
        target_mode = 0 if cmd.get('target') == 'demo' else 2 if cmd.get('target') == 'live' else -1
        current_account = mt5.account_info()
        if (account_identity(current_account) != expected(cmd)
                or getattr(current_account, 'trade_mode', None) != target_mode):
            return result
"""
fill_old = """            volume = sum(float(getattr(d, 'volume', 0)) for d in matches)
            if not math.isfinite(volume) or volume <= 0:
                return result
            state = 'partial' if volume + 1e-8 < submitted['volume'] else 'filled'
"""
fill_new = """            # MT5 deals represent individual immutable fills. Never invent
            # filled volume from corrupt history, duplicate deal IDs, or a
            # volume greater than the durable submitted request.
            try:
                tickets = [int(getattr(d, 'ticket', 0)) for d in matches]
                volumes = [float(getattr(d, 'volume', 0)) for d in matches]
                submitted_volume = float(submitted['volume'])
            except (TypeError, ValueError, KeyError, OverflowError):
                return dict(result, reason='Invalid broker history deal geometry; reconciliation pending')
            if any(t <= 0 for t in tickets) or len(set(tickets)) != len(tickets):
                return dict(result, reason='Duplicate or invalid broker deal tickets; reconciliation pending')
            if not (math.isfinite(submitted_volume) and submitted_volume > 0
                    and all(math.isfinite(v) and v > 0 for v in volumes)):
                return dict(result, reason='Invalid broker history deal volumes; reconciliation pending')
            volume = sum(volumes)
            if not math.isfinite(volume) or volume > submitted_volume + max(1e-8, submitted_volume * 1e-7):
                return dict(result, reason='Broker fill volume exceeds submitted volume; reconciliation pending')
            state = 'partial' if volume + 1e-8 < submitted_volume else 'filled'
"""
if s.count(patched) == 1 and s.count(fill_new) == 1 and s.count(acct_new) == 2:
    print("P0-B reconciliation protections already present")
else:
    checks = ((needle, 1), (acct_old, 2), (fill_old, 1))
    for text, expected in checks:
        if s.count(text) != expected:
            raise SystemExit("Unrecognized original broker reconciliation; refusing blind patch")
    s = s.replace(needle, patched, 1).replace(acct_old, acct_new).replace(fill_old, fill_new, 1)
    p.write_text(s, encoding="utf-8")
print("P0-B strict fill recovery source available for Windows build")
