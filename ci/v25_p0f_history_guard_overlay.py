"""Restore narrow P0-F history/STOP guards atop the tested V24 CI assembly.

Only exact single-occurrence anchors are accepted. This overlay cannot send
broker orders. It never touches user data, V24 branch, or main.
"""
from pathlib import Path
import subprocess

def tracked_file(path):
    return subprocess.check_output(
        ["git","show",f"HEAD:{path}"],text=True,encoding="utf-8")

def between(src,start,end):
    if src.count(start)!=1:
        raise SystemExit("P0-F tracked start anchor ambiguous: "+start[:75])
    pos=src.index(start)
    tail=src.find(end,pos+len(start))
    if tail<0:
        raise SystemExit("P0-F tracked end anchor unavailable: "+end[:75])
    return src[pos:tail]

def exact_replace(src,old,new,label):
    if src.count(old)!=1:
        raise SystemExit(f"P0-F source drift in {label}; refusing blind patch")
    return src.replace(old,new,1)

gateway=Path("gateway/execution.py")
s=gateway.read_text(encoding="utf-8")
tracked=tracked_file("gateway/execution.py")
start="            wanted=(identity['login'],identity['server'],identity['currency'])"
end="        return work.get('poll_ms',250)/1000"
old=between(s,start,end)
new=between(tracked,
    "            # Broker sync is an attribution/learning input.",end)
s=exact_replace(s,old,new,"Python broker history reader")
gateway.write_text(s,encoding="utf-8")

path=Path("crates/runtime/src/execution_legacy.rs")
s=path.read_text(encoding="utf-8")
tracked=tracked_file(str(path).replace("\\","/"))
s=exact_replace(s,
    '"account":{"login":t.login,"server":t.server,"currency":t.currency},"poll_ms"',
    '"account":{"login":t.login,"server":t.server,"currency":t.currency,"trade_mode":t.account_trade_mode},"poll_ms"',
    "Rust broker identity response")
start='                if v["login"] != t.login || v["server"] != t.server || v["currency"] != t.currency {'
end="                let tx = db.unchecked_transaction()?;"
s=exact_replace(s,between(s,start,end),between(tracked,start.replace(' {',''),end),
    "Rust broker history guard")
s=exact_replace(s,'''                self.history_until_ms = v["until_ms"]
                    .as_u64()
                    .filter(|v| *v <= now + 2000)
                    .context("Invalid history boundary")?;
''','''                self.history_until_ms = until_ms;
''',"Rust history boundary committed state")

rust_tests=between(tracked,
    "    #[test]\n    fn stop_keeps_owned_close_authorized_but_cancels_new_open() {",
    "    #[test]\n    fn real_micro_risk_is_consented_latched_by_broker_account_and_restored() {")
anchor="    #[test]\n    fn real_micro_risk_is_consented_latched_by_broker_account_and_restored() {"
s=exact_replace(s,anchor,rust_tests+anchor,"Rust STOP and sync safety tests")
path.write_text(s,encoding="utf-8")
print("P0-F exact Python+Core broker history and STOP guards applied")
