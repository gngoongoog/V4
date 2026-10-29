"""Package only the freshly built canonical V25 Windows application."""
from pathlib import Path
import hashlib,json,shutil,subprocess,sys,zipfile
from release_identity import identity, validate_versions
ROOT=Path(__file__).resolve().parents[1]
DEST=ROOT/'target/portable/BRAINTRADE_V4_ENTERPRISE_V25'
MARKER=json.loads((ROOT/'release.json').read_text('utf-8'))['marker'].encode('ascii')
V24='9e8011178c581bc89dcb45f62de7320abd1031dfa4f597425ed2c64d33b64d14'
P0N='e08efc5607e56be74b919e789414d60543283752c4c8eccd7d4467501d354ef8'
def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def stage():
    exe=ROOT/'target/release/BrainTradeNext.exe'
    validate_versions()
    expected=identity()
    observed=json.loads(subprocess.check_output([str(exe),'--braintrade-build-id'],text=True))
    if observed!=expected:raise ValueError('Executable source identity does not match current application sources')
    data=exe.read_bytes()
    if data[:2]!=b'MZ' or MARKER not in data or digest(exe) in (V24,P0N):raise ValueError('Expected newly compiled V25 RC2 Windows EXE')
    if DEST.exists():shutil.rmtree(DEST)
    DEST.mkdir(parents=True)
    shutil.copy2(exe,DEST/exe.name)
    for name in ('runtime','mt5'):shutil.copytree(ROOT/'apps/tauri'/name,DEST/name)
    (DEST/'Tools').mkdir();shutil.copy2(ROOT/'scripts/v25_verify_system.py',DEST/'Tools/v25_verify_system.py')
    for name in ('VERIFY_SYSTEM.cmd','V25_P0_DIAGNOSTICS_README_AR.txt'):
        shutil.copy2(ROOT/name,DEST/name)
    shutil.copy2(ROOT/'scripts/Create-V25-Shortcut.ps1',DEST/'DESKTOP_SHORTCUT.ps1')
    # P0-B/G2: ship the actual read-only Demo Acceptance Center described by
    # DEMO_ACCEPTANCE_AR.md. Source-tree tools alone do not help a Portable user.
    shutil.copy2(ROOT/'scripts/Run-Demo-Acceptance.cmd',DEST/'Run-Demo-Acceptance.cmd')
    for item in ('Run-Demo-Acceptance.ps1','Run-Demo-Acceptance-GUI.ps1'):
        shutil.copy2(ROOT/'scripts'/item,DEST/'Tools'/item)
    shutil.copy2(ROOT/'docs/DEMO_ACCEPTANCE_AR.md',DEST/'DEMO_ACCEPTANCE_AR.md')
    shutil.copy2(ROOT/'docs/v25/G2_DEMO_READINESS_PREFLIGHT_AR.md',DEST/'G2_DEMO_READINESS_AR.md')
    # G6-C: research collectors must be available in the real Portable,
    # not only the source archive. Read-only Core SQLite, NEVER MT5 orders.
    study=DEST/'Tools/G6'
    study.mkdir(parents=True,exist_ok=True)
    for name in ('audit_v25_g5d_frozen_pairs.py',
                 'audit_v25_g6a_preregistered_quotes.py',
                 'audit_v25_g6b_executed_demo_arm.py',
                 'audit_v25_g6d_causal_learning_evidence.py',
                 'audit_v25_g6e_frozen_policy_cutoff.py',
                 'audit_v25_g6g_broker_admission_clock.py',
                 'v25_g6c_list_frozen_runs.py'):
        shutil.copy2(ROOT/'scripts'/name,study/name)
    shutil.copy2(ROOT/'scripts/Run-G6-Study.ps1',DEST/'Tools/Run-G6-Study.ps1')
    shutil.copy2(ROOT/'scripts/Run-G6-Study.cmd',DEST/'Run-G6-Study.cmd')
    shutil.copy2(ROOT/'docs/v25/G6C_PORTABLE_STUDY_OPERATOR_AR.md',DEST/'G6_STUDY_AR.md')
    # G7-H: ship full source-auditing chain inside the actual Portable,
    # including local G6-A/B dependencies. Nothing here executes MT5 orders.
    study7=DEST/'Tools/G7'
    study7.mkdir(parents=True,exist_ok=True)
    for name in (
        'g7_core_source_audit.py','g7_demo_broker_lineage.py',
        'g7_full_intent_census.py','g7_cohort_overlap_gate.py',
        'g7_holdout_report.py','g7f_sealed_candidate_audit.py',
        'g7g_restart_continuity.py','g7_demo_acceptance_packet.py',
        'audit_v25_g5d_frozen_pairs.py',
        'audit_v25_g6a_preregistered_quotes.py',
        'audit_v25_g6b_executed_demo_arm.py',
    ):
        shutil.copy2(ROOT/'scripts'/name,study7/name)
    shutil.copy2(ROOT/'scripts/Run-G7-Acceptance.ps1',DEST/'Tools/Run-G7-Acceptance.ps1')
    shutil.copy2(ROOT/'scripts/Run-G7-Acceptance.cmd',DEST/'Run-G7-Acceptance.cmd')
    shutil.copy2(ROOT/'docs/v25/G7H_NATIVE_DEMO_ACCEPTANCE_AR.md',DEST/'G7_H_ACCEPTANCE_AR.md')
    shutil.copy2(ROOT/'docs/v25/USER_MANUAL_V25_AR.md',DEST/'USER_MANUAL_V25_AR.md')
    shutil.copy2(ROOT/'docs/v25/USER_MANUAL_V25_AR.md',DEST/'USER_MANUAL_V25_AR.txt')
    shutil.copy2(ROOT/'docs/v25/CONCEPT4_UI_AR.md',DEST/'CONCEPT4_UI_AR.md')
    # Redacted reports are written under Documents, outside the signed tree.
    # P0-E: ship source + a strictly explicit operator-side MetaEditor compile
    # path. Build CI has no installed MT5 and must never claim EA .ex5 passed.
    shutil.copy2(ROOT/'scripts/Compile-Sniper-EA.cmd',DEST/'Compile-Sniper-EA.cmd')
    shutil.copy2(ROOT/'scripts/Compile-Sniper-EA.ps1',DEST/'Tools/Compile-Sniper-EA.ps1')
    shutil.copy2(ROOT/'docs/v25/NATIVE_EA_ACCEPTANCE_AR.md',DEST/'NATIVE_EA_ACCEPTANCE_AR.md')
    for name,flag in [('CREATE_DESKTOP_ICON.cmd',''),('REMOVE_DESKTOP_ICON.cmd',' -Remove')]:
        (DEST/name).write_bytes(('@echo off\r\npowershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0DESKTOP_SHORTCUT.ps1"'+flag+'\r\nif errorlevel 1 exit /b 1\r\npause\r\n').encode('ascii'))
    evidence=DEST/'release-evidence';evidence.mkdir()
    for source,name in [('Cargo.lock','Cargo.lock'),('apps/terminal/package-lock.json','package-lock.json'),('docs/v25/P0O_PROVENANCE.json','P0O_PROVENANCE.json')]:
        shutil.copy2(ROOT/source,evidence/name)
    commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    (evidence/'build.json').write_text(json.dumps({**expected,'source_commit':commit,'exe_sha256':digest(exe),'dependency_resolution':'committed locks; cargo --locked; npm ci','broker_demo':'NOT TESTED','windows_gui_all_controls':'NOT TESTED','ea_compile':'NOT TESTED','ready_for_real':False},indent=2)+'\n','utf-8')
    (DEST/'READ_ME_FIRST_AR.txt').write_text('BRAINTRADE V4 Enterprise V25 RC2 — 0.25.1\nفك المجلد كاملاً وشغّل BrainTradeNext.exe مباشرة. لا يوجد Setup إلزامي.\nيتطلب Windows x64 وWebView2. يحتاج التداول إلى MT5 وحسابك التجريبي.\nلإنشاء اختصار سطح المكتب شغّل CREATE_DESKTOP_ICON.cmd.\nضع مسار terminal64.exe من صفحة الإعدادات، ثم اربط DEMO من صفحة التنفيذ.\nشغّل VERIFY_SYSTEM.cmd لإنتاج تقرير HTML وJSON؛ هذه الأداة لا تفتح صفقات ولا ترسل بيانات.\nلجمع نتائج تنفيذ DEMO للقراءة فقط شغّل Run-Demo-Acceptance.cmd؛ لا يفعل START أو يرسل أوامر MT5.\nلإعداد تجربة G6 ومراجعة أسعار Demo ونتائج الوسيط الحقيقية شغّل Run-G6-Study.cmd واقرأ G6_STUDY_AR.md؛ لا يفتح MT5 أو أوامر.\nلترجمة Native EA على جهاز فيه MetaEditor افتح Compile-Sniper-EA.cmd واقرأ NATIVE_EA_ACCEPTANCE_AR.md؛ لا يوجد EX5 مثبت من CI.\nاقرأ USER_MANUAL_V25_AR.txt أو USER_MANUAL_V25_AR.md لجميع أوضاع التداول وصفحات V25.\nللتدقيق البحثي G7-H افتح Run-G7-Acceptance.cmd بعد وجود بيانات دراسة؛ لا يفتح صفقات.\nهذه نسخة تحقق مرحلية: اختبار MT5 Demo وEA وجميع أزرار Windows لم يُعتمد بعد.\nSTOP يمنع الدخول الجديد ويبقي إدارة المراكز؛ لا تغلق Core مع صفقات أو أوامر غير محسومة.\nلا تنقل EXE وحده ولا تحذف بيانات التطبيق عند التحديث. احتفظ بحزمة V24 للرجوع.\n','utf-8')
def archive():
    shutil.copytree(ROOT/'test-output/v25',DEST/'release-evidence/validation',dirs_exist_ok=True)
    manifest={p.relative_to(DEST).as_posix():digest(p) for p in sorted(DEST.rglob('*')) if p.is_file() and p.name!='SHA256SUMS.json'}
    (DEST/'SHA256SUMS.json').write_text(json.dumps(manifest,indent=2)+'\n','utf-8')
    output=DEST.parent/'BRAINTRADE_V4_ENTERPRISE_V25_PORTABLE_Windows_x64.zip'
    with zipfile.ZipFile(output,'w',zipfile.ZIP_DEFLATED,compresslevel=6) as z:
        for p in sorted(DEST.rglob('*')):
            if p.is_file():z.write(p,p.relative_to(DEST.parent))
    with zipfile.ZipFile(output) as z:
        if z.testzip():raise ValueError('ZIP integrity failed')
    print(json.dumps({'archive':output.name,'sha256':digest(output),'files':len(manifest)}))
if __name__=='__main__':
    if sys.argv[1:]==['stage']:stage()
    elif sys.argv[1:]==['archive']:archive()
    else:raise SystemExit('Usage: package_v25.py stage|archive')
