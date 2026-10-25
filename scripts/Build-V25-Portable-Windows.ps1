# Build the committed source directly. No CI source restoration or code patches.
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$results=[System.Collections.Generic.List[object]]::new()
function Run([string]$Exe,[string[]]$Arguments) {
  & $Exe @Arguments
  $code=$LASTEXITCODE
  $results.Add(@{command=$Exe;arguments=$Arguments;exit_code=$code;status=$(if($code -eq 0){'PASS'}else{'FAIL'})})
  if($code -ne 0){throw "$Exe failed ($code)"}
}
Push-Location $root
$transcribing=$false
try {
  $env:PYTHONDONTWRITEBYTECODE='1'
  New-Item -Force -ItemType Directory 'test-output/v25' | Out-Null
  Start-Transcript -Path 'test-output/v25/windows-build.txt' -Force
  $transcribing=$true
  Run npm @('--prefix','apps/terminal','ci','--no-audit','--no-fund')
  Run npm @('--prefix','apps/terminal','test')
  Run npm @('--prefix','apps/terminal','run','build')
  Run python @('scripts/prepare-tauri-runtime.py')
  Run cargo @('test','--locked','-p','braintrade-domain','-p','braintrade-runtime','-p','braintrade-tauri')
  Run python @('-m','unittest','discover','-s','tests','-v')
  Run python @('scripts/audit_v25_windows_pages.py','--output','test-output/v25/windows-page-matrix.json')
  # custom-protocol embeds the production frontend; no globally installed CLI.
  Run cargo @('build','--release','--locked','-p','braintrade-tauri','--bin','BrainTradeNext')
  Run git @('diff','--exit-code','--','Cargo.lock','apps/terminal/package-lock.json')
  Run python @('scripts/package_v25.py','stage')
  $exe=Join-Path $root 'target/portable/BRAINTRADE_V4_ENTERPRISE_V25/BrainTradeNext.exe'
  $portable=Split-Path $exe
  # P0-B/G2: never advertise the Acceptance Center unless its launcher,
  # restricted read-only collector, CLI, GUI and Arabic manual are all staged.
  Run python @('scripts/check_v25_acceptance_package.py','--portable',$portable,
    '--output','test-output/v25/demo-acceptance-package.json')
  Run python @('scripts/check_v25_native_ea_package.py','--portable',$portable,
    '--output','test-output/v25/native-ea-package.json')
  Run python @('scripts/check_v25_g6c_study_package.py','--portable',$portable,
    '--output','test-output/v25/g6c-study-package.json')
  # Validate the actual shipped Windows study launcher syntax (not only
  # Python help/sha checks). No Core DB or MT5 is opened here.
  $studyTokens=$null
  $studyErrors=$null
  [System.Management.Automation.Language.Parser]::ParseFile(
    (Join-Path $portable 'Tools/Run-G6-Study.ps1'),
    [ref]$studyTokens,[ref]$studyErrors) | Out-Null
  if($studyErrors.Count -gt 0){
    throw ('G6 Portable PowerShell study launcher invalid: '+$studyErrors[0].Message)
  }
  Run (Join-Path $portable 'runtime/python/python.exe') @((Join-Path $portable 'runtime/gateway/check_runtime.py'))
  Run python @('scripts/smoke_portable_core.py',$exe)
  Run python @('scripts/check_sniper_quote_clock.py','--exe',$exe)
  # Relocation and spaces must not make Core resolve Gateway relative to cwd.
  $moved=Join-Path $root 'target/portable/Relocated V25 Space'
  if(Test-Path $moved){Remove-Item $moved -Recurse -Force}
  Move-Item $portable $moved
  Run python @('scripts/check_v25_acceptance_package.py','--portable',$moved,
    '--output','test-output/v25/demo-acceptance-relocated.json')
  Run python @('scripts/check_v25_native_ea_package.py','--portable',$moved,
    '--output','test-output/v25/native-ea-relocated.json')
  Run python @('scripts/check_v25_g6c_study_package.py','--portable',$moved,
    '--output','test-output/v25/g6c-study-relocated.json')
  Run python @('scripts/smoke_portable_core.py',(Join-Path $moved 'BrainTradeNext.exe'))
  Move-Item $moved $portable
  & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $portable 'DESKTOP_SHORTCUT.ps1')
  if($LASTEXITCODE -ne 0){throw 'Shortcut creation failed'}
  $shortcut=Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) 'BRAINTRADE V4 V25.lnk'
  $link=(New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)
  if($link.TargetPath -ne $exe -or $link.WorkingDirectory -ne $portable){throw 'Shortcut target mismatch'}
  & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $portable 'DESKTOP_SHORTCUT.ps1') -Remove
  if($LASTEXITCODE -ne 0){throw 'Shortcut cleanup failed'}
  $results.Add(@{command='Windows Desktop shortcut target';status='PASS';exit_code=0})
  Stop-Transcript
  $transcribing=$false
  [IO.File]::WriteAllText((Join-Path $root 'test-output/v25/results.json'),($results|ConvertTo-Json -Depth 6),[Text.Encoding]::UTF8)
  Run python @('scripts/package_v25.py','archive')
  Run python @('scripts/v25_verify_system.py','--portable',$portable,'--output',(Join-Path $root 'test-output/v25/package-verification'))
  Move-Item $portable $moved
  Run cmd.exe @('/d','/c','call',(Join-Path $moved 'VERIFY_SYSTEM.cmd'),'--ci')
  Copy-Item (Join-Path $moved 'verification-output') (Join-Path $root 'test-output/v25/launcher-in-spaces') -Recurse -Force
  Move-Item $moved $portable
} finally {
  if($transcribing){Stop-Transcript}
  [IO.File]::WriteAllText((Join-Path $root 'test-output/v25/results.json'),($results|ConvertTo-Json -Depth 6),[Text.Encoding]::UTF8)
  Pop-Location
}
