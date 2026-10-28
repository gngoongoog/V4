# Compile this exact source with an official, signature-verified MetaEditor on
# an ephemeral Windows test runner. No MT5 account or broker order is used.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$evidence = Join-Path $root 'test-output/native-ea-compile'
$install = Join-Path $env:RUNNER_TEMP 'braintrade-ci-mt5'
$setup = Join-Path $env:RUNNER_TEMP 'official-mt5setup.exe'
New-Item -ItemType Directory -Force -Path $evidence | Out-Null

$url = 'https://download.terminal.free/cdn/web/metaquotes.ltd/mt5/mt5setup.exe'
Invoke-WebRequest -Uri $url -OutFile $setup -TimeoutSec 120
$signature = Get-AuthenticodeSignature -LiteralPath $setup
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'MetaQuotes') {
  throw 'Official MT5 setup signature was not verified'
}
Write-Host ('MT5 setup signature: ' + $signature.Status + '; ' + $signature.SignerCertificate.Subject)
$installer = Start-Process -FilePath $setup -ArgumentList @('/auto',('/path:"' + $install + '"')) -PassThru
if (-not $installer.WaitForExit(600000)) {
  Stop-Process -Id $installer.Id -Force -ErrorAction SilentlyContinue
  throw 'Official MT5 installer timed out'
}
$editor = Join-Path $install 'metaeditor64.exe'
if (-not (Test-Path -LiteralPath $editor -PathType Leaf)) {
  throw ('Official installer did not produce MetaEditor; exit=' + $installer.ExitCode)
}
$editorSignature = Get-AuthenticodeSignature -LiteralPath $editor
if ($editorSignature.Status -ne 'Valid' -or $editorSignature.SignerCertificate.Subject -notmatch 'MetaQuotes') {
  throw 'Installed MetaEditor signature was not verified'
}
# The installer may launch the terminal after installation. Do not log in or
# execute an EA, and close only the isolated terminal under this test path.
Get-CimInstance Win32_Process -Filter "Name = 'terminal64.exe'" |
  Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($install, [StringComparison]::OrdinalIgnoreCase) } |
  ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }

# Exercise the actual operator helper from a relocated Portable-style folder.
$portable = Join-Path $env:RUNNER_TEMP 'BrainTrade EA Portable With Spaces'
$data = Join-Path $env:RUNNER_TEMP 'MT5 Data With Spaces'
$tools = Join-Path $portable 'Tools'
$sourceDir = Join-Path $portable 'mt5/Experts'
New-Item -ItemType Directory -Force -Path $tools,$sourceDir,(Join-Path $data 'MQL5/Experts') | Out-Null
$source = Join-Path $sourceDir 'BrainTradeSniperBridge.mq5'
Copy-Item -LiteralPath (Join-Path $root 'mt5/BrainTradeSniperBridge.mq5') -Destination $source
$helper = Join-Path $tools 'Compile-Sniper-EA.ps1'
Copy-Item -LiteralPath (Join-Path $root 'scripts/Compile-Sniper-EA.ps1') -Destination $helper
$stdout = Join-Path $evidence 'helper-output.txt'
$stderr = Join-Path $evidence 'helper-errors.txt'
$arguments = @('-NoProfile','-File',('"' + $helper + '"'),
  '-MetaEditorPath',('"' + $editor + '"'),'-TerminalDataDir',('"' + $data + '"'))
$runner = Start-Process -FilePath (Join-Path $PSHOME 'pwsh.exe') -ArgumentList $arguments -PassThru `
  -RedirectStandardOutput $stdout -RedirectStandardError $stderr
if (-not $runner.WaitForExit(150000)) {
  Stop-Process -Id $runner.Id -Force -ErrorAction SilentlyContinue
  throw 'Portable MetaEditor helper timed out'
}
if (Test-Path -LiteralPath $stdout) { Get-Content -LiteralPath $stdout | Write-Host }
if (Test-Path -LiteralPath $stderr) { Get-Content -LiteralPath $stderr | Write-Host }
$helperEvidence = Join-Path $portable 'acceptance-output/native-ea'
if (Test-Path -LiteralPath $helperEvidence) {
  Copy-Item -Path (Join-Path $helperEvidence '*') -Destination $evidence -Force
}
$compiled = Join-Path $data 'MQL5/Experts/BrainTradeSniperBridge.ex5'
$helperReport = Join-Path $helperEvidence 'metaeditor-compile.json'
if (Test-Path -LiteralPath $compiled -PathType Leaf) {
  Copy-Item -LiteralPath $compiled -Destination $evidence -Force
}
if (Test-Path -LiteralPath $helperReport -PathType Leaf) {
  Get-Content -LiteralPath $helperReport -Raw | Write-Host
}
if ($runner.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $compiled -PathType Leaf) -or
    -not (Test-Path -LiteralPath $helperReport -PathType Leaf)) {
  throw ('Portable helper failed to compile EA; exit=' + $runner.ExitCode)
}
$validated = Get-Content -LiteralPath $helperReport -Raw | ConvertFrom-Json
if ($validated.metaeditor_compiled -ne 'PASS' -or $validated.ready_for_real -ne $false) {
  throw 'Portable helper did not produce a successful, explicitly non-LIVE compile report'
}
$logs = @(Get-ChildItem -LiteralPath $helperEvidence -Filter 'metaeditor-*.log' -File)
if ($logs.Count -ne 1) { throw 'Expected exactly one new MetaEditor log' }
$logText = Get-Content -LiteralPath $logs[0].FullName -Raw
Write-Host $logText
if ($logText -notmatch '\b0\s+errors?\b' -or $logText -notmatch '\b0\s+warnings?\b') {
  throw 'Official MetaEditor did not report zero errors and warnings'
}
Copy-Item -LiteralPath $source -Destination $evidence -Force
Copy-Item -LiteralPath $compiled -Destination $evidence -Force
$report = [ordered]@{
  schema = 'braintrade-native-ci-compile-v1'
  source_commit = (& git -C $root rev-parse HEAD).Trim()
  source_sha256 = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
  ex5_sha256 = (Get-FileHash -LiteralPath $compiled -Algorithm SHA256).Hash.ToLowerInvariant()
  editor_file_version = (Get-Item -LiteralPath $editor).VersionInfo.FileVersion
  installer_signature = [string]$signature.Status
  editor_signature = [string]$editorSignature.Status
  metaeditor_compile = 'PASS'
  real_mt5_demo_tick = 'NOT TESTED'
  real_broker_trade = 'NOT TESTED'
  ready_for_real = $false
}
$report | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $evidence 'compile-result.json') -Encoding UTF8
# Reuse the same operator helper with deliberately invalid test-only source.
# The prior successful EX5 must never turn this failure into a reported PASS.
Set-Content -LiteralPath $source -Value 'this is deliberately invalid MQL5 source' -Encoding ASCII
$negativeOut = Join-Path $evidence 'invalid-source-output.txt'
$negativeErr = Join-Path $evidence 'invalid-source-errors.txt'
$negative = Start-Process -FilePath (Join-Path $PSHOME 'pwsh.exe') -ArgumentList $arguments -PassThru `
  -RedirectStandardOutput $negativeOut -RedirectStandardError $negativeErr
if (-not $negative.WaitForExit(150000)) {
  Stop-Process -Id $negative.Id -Force -ErrorAction SilentlyContinue
  throw 'Invalid-source compile check timed out'
}
$negativeReport = Get-Content -LiteralPath $helperReport -Raw | ConvertFrom-Json
$negativeReport | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $evidence 'invalid-source-result.json') -Encoding UTF8
if ($negative.ExitCode -eq 0 -or $negativeReport.metaeditor_compiled -ne 'FAIL') {
  throw 'Portable helper incorrectly accepted invalid source with an existing EX5'
}
Write-Host 'PASS: deliberately invalid source rejected even with an earlier successful EX5.'
Write-Host 'PASS: signed official MetaEditor compiled current MQ5; MT5 Demo remains NOT TESTED.'
