"""Conservative UI18 source/release completeness gate; NEVER certify live trading.
Run: python scripts/verify_ui18_source.py [--release]
"""
from __future__ import annotations
import argparse
from pathlib import Path
import json
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
NEEDED = [
    'Cargo.toml','Cargo.lock','crates/runtime/Cargo.toml',
    'crates/runtime/src/lib.rs','crates/runtime/src/execution.rs',
    'crates/runtime/src/brain_control.rs','crates/domain/src/lib.rs',
    'gateway/mt5_gateway.py','gateway/execution.py',
    'mt5/BrainTradeSniperBridge.mq5',
    'apps/tauri/Cargo.toml','apps/tauri/src/main.rs',
    'apps/tauri/src/bridge.rs','apps/tauri/tauri.conf.json',
    'apps/terminal/src/main.tsx','apps/terminal/src/terminal.tsx',
    'apps/terminal/src/operations.tsx','apps/terminal/src/feed.ts',
    'apps/terminal/package.json',
    'scripts/Build-Tauri-Windows.ps1','scripts/prepare-tauri-runtime.py',
    'tests/test_backup_journal_boundary.py',
    '.github/workflows/tauri-windows.yml',
]


def validate(strict: bool = False):
    errors = []
    warnings = []
    for path in NEEDED:
        if not (ROOT / path).is_file():
            errors.append(f'Critical source file missing: {path}')
    if errors:
        return errors, warnings
    rust = (ROOT / 'crates/runtime/src/lib.rs').read_text('utf-8')
    bridge = (ROOT / 'apps/tauri/src/bridge.rs').read_text('utf-8')
    js = (ROOT / 'apps/terminal/src/terminal.tsx').read_text('utf-8')
    operations = (ROOT / 'apps/terminal/src/operations.tsx').read_text('utf-8')
    checks = {
        'core shutdown safety check': 'verify_safe_shutdown(&current,unresolved)?' in rust,
        'core config stops / positions gate': 'verify_safe_reconfiguration(&current,pending)?' in rust,
        'core confirms pending broker commands': 'self.execution.pending(&self.db)?' in rust,
        'core rejects open positions for shutdown': 't.positions.is_empty() && t.pending_orders == 0' in rust,
        'tauri no arbitrary HTTP proxy': 'allowed_read_route(&route)' in bridge,
        'tauri post-call full exit': 'open_client()?.post("/v1/shutdown"' in bridge,
        'tauri prevents core worker relaunch': 'lifecycle.stopping.store(true,Ordering::SeqCst)' in bridge,
        'backup bound to Core': '"/v1/backup"' in bridge,
        'brain snapshot bound to Core': '"/v1/brain-snapshot/export"' in bridge,
        'backtest Core start': '"/v1/backtest/start"' in bridge,
        'brain controls Core route': '"/v1/execution/brain-control"' in bridge,
        'broker binding Core route': '"/v1/execution/settings"' in bridge and 'fn bind_execution(' in bridge,
        'Tauri explicitly authorizes REAL account number': 'confirm_live_login' in bridge and 'account_trade_mode' in (ROOT / 'crates/runtime/src/execution_legacy.rs').read_text('utf-8'),
        'no secondary live toggle': '"system"' in bridge and 'advisor' in bridge,
        'mode/commission Core validation': '"/v1/trading/settings"' in bridge,
        'stop new entries confirmation': 'showStopConfirm&&' in js and 'تأكيد وقف الدخول الجديد' in js,
        'separate safe exit and close UI': 'showExitConfirm&&' in js and 'إغلاق الواجهة فقط' in js,
        'real backtest form': 'BacktestOperations' in js and 'onStart' in operations,
        'real backup form': 'BackupOperations' in js,
        'real brain controls': 'BrainControlsOperations' in js,
        'real coordinator settings': 'TradingSettingsOperations' in js,
        'broker account bind UI': 'ExecutionBindingOperations' in js and 'runBrokerBind' in js,
        'browser fixtures separated': 'const display=preview?DESIGN_FIXTURE:status' in js,
    }
    errors += [f'Operational source gate failed: {name}' for name, ok in checks.items() if not ok]
    # Artifact-specific facts that cannot be bypassed with source assertions.
    package_lock = ROOT / 'apps/terminal/package-lock.json'
    if not package_lock.is_file():
        warnings.append('npm lock missing: dependencies are not fully pinned')
        if strict:
            errors.append('Release gate: generate and commit package-lock.json using npm install on networked builder')
    cargo_lock = (ROOT/'Cargo.lock').read_text('utf-8')
    if not re.search(r'^name = "tauri"$',cargo_lock,re.MULTILINE):
        warnings.append('Cargo.lock missing Tauri 2 resolution: native build must resolve and commit lock on Windows')
        if strict:
            errors.append('Release gate: Tauri 2 must appear in committed Cargo.lock')
    if strict and not list((ROOT/'target/release/bundle/nsis').glob('*-setup.exe')):
        errors.append('Release gate: Windows NSIS installer not built here')
    if strict and not (ROOT/'test-output/demo-acceptance/acceptance.json').is_file():
        errors.append('Release gate: MT5 Demo target-device acceptance evidence missing')
    return errors,warnings


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--release',action='store_true',help='fail closed unless native installer, locks and Demo acceptance exist')
    args=parser.parse_args()
    errors,warnings=validate(args.release)
    result={
        'source_integrity':not errors if not args.release else None,
        'release_authorized':args.release and not errors,
        'checks':len(NEEDED),
        'errors':errors,
        'warnings':warnings,
        'scope':'source/operational contract static checks only, not compilation, GUI acceptance or broker execution'
    }
    print(json.dumps(result,ensure_ascii=False,indent=2))
    return 1 if errors else 0


if __name__=='__main__':
    sys.exit(main())
