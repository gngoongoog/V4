param([switch]$SkipCoreTests,[switch]$SkipPackage)
$ErrorActionPreference='Stop'
$Root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $Root
try {
  $env:PYTHONDONTWRITEBYTECODE='1'
  function Run([string]$Exe,[string[]]$ArgsList) {
    & $Exe @ArgsList
    if($LASTEXITCODE -ne 0) { throw "$Exe failed with code $LASTEXITCODE" }
  }
  if(-not (Get-Command rustc -ErrorAction SilentlyContinue)) {throw 'Rust stable MSVC and Visual Studio C++ Build Tools required'}
  if(-not (Get-Command node -ErrorAction SilentlyContinue)) {throw 'Node.js required'}
  if(-not (Get-Command tauri -ErrorAction SilentlyContinue)) {
    Write-Host 'Installing Tauri CLI v2 via npm...'
    Run npm @('install','--global','@tauri-apps/cli@2','--no-audit','--no-fund')
  }
  Run python @('scripts/verify_ui18_source.py')
  Run python @('scripts/verify_ui20_supervisor.py')
  Push-Location (Join-Path $Root 'apps/terminal')
  try {
    Run npm @('install','--no-audit','--no-fund')
    Run npm @('test')
    Run npm @('run','build')
  } finally {Pop-Location}
  if(-not $SkipCoreTests) {
    Run cargo @('test','-p','braintrade-domain','-p','braintrade-runtime')
    Run cargo @('build','-p','braintrade-runtime','--bin','braintrade-core')
    Run python @('-m','unittest','discover','-s','tests','-q')
  }
  if(-not $SkipPackage) {Run python @('scripts/prepare-tauri-runtime.py')}
  Push-Location (Join-Path $Root 'apps/tauri')
  try {Run tauri @('build','--bundles','nsis')}
  finally {Pop-Location}
  Write-Host 'Native V23 build succeeded; MT5 Demo broker acceptance remains separate.'
} finally {Pop-Location}
