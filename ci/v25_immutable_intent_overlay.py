"""Fail closed if restored V24 Gateway cannot accept the P0-C immutable-intent guard."""
from pathlib import Path

path=Path("gateway/execution.py")
content=path.read_text(encoding="utf-8")
old="            if any(saved.get(k)!=cmd.get(k) for k in ('login','server','currency','symbol','magic','kind','position')):\n"
new="""            # Durable intent IDs bind the entire economic decision, not only
            # the broker account. Never reinterpret a saved order with new lot,
            # side, expiry, fees, risk permissions or Demo/REAL destination.
            # Mutable polling state and timing metadata are deliberately omitted.
            immutable=('login','server','currency','target','symbol','magic',
                       'kind','position','lots','signal','revision',
                       'deviation_points','created_ms','expires_ms',
                       'commission_per_lot','slippage_ticks','risk_percent',
                       'demo_flexible','micro_demo_risk_opt_in',
                       'micro_real_temporary','micro_real_cap_percent',
                       'micro_real_recovery_equity','daily_breaker_enabled',
                       'daily_breaker_limit_percent','max_quote_age_ms')
            if any(saved.get(k)!=cmd.get(k) for k in immutable):
"""
if content.count(old)==1:
    path.write_text(content.replace(old,new),encoding="utf-8")
elif content.count(new)==1:
    print("V25 immutable journal scope already installed")
else:
    raise SystemExit("Cannot identify original journal replay identity comparison; refusing to build")
print("V25 immutable intent contract staged")
