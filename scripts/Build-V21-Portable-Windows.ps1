# Full portable from V21 SOURCE, not an old UI18 binary with new labels.
# Run in Windows PowerShell with Node 22, Rust stable MSVC + C++ Build Tools, Python 3.13.
param([switch]$SkipCoreTests)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $root
try {
  & python (Join-Path $PSScriptRoot 'verify_ui21_supervisor.py')
  if ($LASTEXITCODE -ne 0) { throw 'V21 source integrity gate failed' }
  & (Join-Path $PSScriptRoot 'Build-Tauri-Windows.ps1') -SkipCoreTests:$SkipCoreTests
  if ($LASTEXITCODE -ne 0) { throw 'Native V21 build failed' }
  $sourceExe = Join-Path $root 'target/release/BrainTradeNext.exe'
  if (-not (Test-Path $sourceExe)) { throw 'V21 BrainTradeNext.exe not generated' }
  $runtime = Join-Path $root 'apps/tauri/runtime'
  $expert = Join-Path $root 'apps/tauri/mt5'
  foreach($path in @($runtime, $expert)) {
    if (-not (Test-Path $path)) { throw "Runtime/EA bundle not prepared: $path" }
  }
  $dest = Join-Path $root 'target/portable/BRAINTRADE-V4-V21-PORTABLE'
  New-Item -Force -ItemType Directory $dest | Out-Null
  Copy-Item $sourceExe (Join-Path $dest 'BrainTradeNext.exe') -Force
  Copy-Item $runtime (Join-Path $dest 'runtime') -Recurse -Force
  Copy-Item $expert (Join-Path $dest 'mt5') -Recurse -Force
  $must = @('BrainTradeNext.exe','runtime/python/python.exe','runtime/gateway/mt5_gateway.py', 'mt5/Experts/BrainTradeSniperBridge.mq5')
  foreach($rel in $must) {
    $p=Join-Path $dest $rel
    if(-not (Test-Path $p)) {throw "Incomplete portable package: $rel"}
  }
  $start = "@echo off`r`ncd /d `"%~dp0`"`r`nstart `"`" `"%~dp0BrainTradeNext.exe`"`r`n"
  [IO.File]::WriteAllText((Join-Path $dest 'START_BRAINTRADE.cmd'),$start,[Text.Encoding]::ASCII)
  $note = @'
BRAINTRADE V4 • V21 SOURCE BUILD
- Extract the complete archive and run START_BRAINTRADE.cmd.
- MT5 terminal and Microsoft WebView2 must be present on the Windows machine.
- V21 supervisor is advisory by default. Admission blocking is opt-in only.
- Test on Demo before any consideration of live trading.
- STOP blocks new entries but does not close existing broker positions.
- Portable verification is only valid for binaries compiled from this V21 source.
'@
  [IO.File]::WriteAllText((Join-Path $dest 'READ_ME_FIRST.txt'),$note,[Text.Encoding]::UTF8)
  $python = Join-Path $dest 'runtime/python/python.exe'
  $check = Join-Path $dest 'runtime/gateway/check_runtime.py'
  if (Test-Path $check) {
    & $python $check
    if ($LASTEXITCODE -ne 0) {throw 'Embedded Python/MT5 runtime smoke test failed'}
  }
  $manifest = @{}
  foreach($file in @(Get-ChildItem $dest -File -Recurse)) {
    $relative = $file.FullName.Substring($dest.Length+1).Replace('\','/')
    $manifest[$relative] = (Get-FileHash -Algorithm SHA256 $file.FullName).Hash.ToLowerInvariant()
  }
  [IO.File]::WriteAllText((Join-Path $dest 'SHA256SUMS.json'),($manifest|ConvertTo-Json -Depth 3),[Text.Encoding]::UTF8)
  $zip=Join-Path $root 'target/portable/BRAINTRADE-V4-V21-PORTABLE-Win64.zip'
  Compress-Archive -Path $dest -DestinationPath $zip -Force -CompressionLevel Optimal
  Write-Host "Portable V21 package built: $zip"
  Write-Host 'You must still complete Windows GUI and real MT5 Demo end-to-end acceptance.'
}
finally {Pop-Location}
