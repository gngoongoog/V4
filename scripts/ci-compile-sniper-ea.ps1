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

$source = Join-Path $evidence 'BrainTradeSniperBridge.mq5'
$compiled = Join-Path $evidence 'BrainTradeSniperBridge.ex5'
$log = Join-Path $evidence 'metaeditor.log'
Copy-Item -LiteralPath (Join-Path $root 'mt5/BrainTradeSniperBridge.mq5') -Destination $source
$started = [DateTime]::UtcNow
& $editor ('/compile:' + $source) ('/log:' + $log)
$exitCode = $LASTEXITCODE
if (-not (Test-Path -LiteralPath $compiled -PathType Leaf) -or
    -not (Test-Path -LiteralPath $log -PathType Leaf)) {
  throw ('MetaEditor did not produce both EX5 and log; exit=' + $exitCode)
}
$built = Get-Item -LiteralPath $compiled
$logText = Get-Content -LiteralPath $log -Raw
Write-Host $logText
if ($exitCode -ne 0 -or $built.Length -le 1024 -or $built.LastWriteTimeUtc -lt $started.AddSeconds(-2) -or
    $logText -notmatch '\b0\s+errors?\b' -or $logText -notmatch '\b0\s+warnings?\b') {
  throw ('MetaEditor build was not clean or fresh; exit=' + $exitCode)
}
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
Write-Host 'PASS: signed official MetaEditor compiled current MQ5; MT5 Demo remains NOT TESTED.'
