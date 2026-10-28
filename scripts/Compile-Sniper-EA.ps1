# P0-E operator-only MetaEditor compilation. No MT5 connection or trading calls.
# InspectOnly is the default behavior without explicit compiler+MT5 data-folder paths.
[CmdletBinding()]
param(
  [string]$MetaEditorPath = '',
  [string]$TerminalDataDir = '',
  [switch]$InspectOnly
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$source = Join-Path $root 'mt5\Experts\BrainTradeSniperBridge.mq5'
$reportDir = Join-Path $root 'acceptance-output\native-ea'
$reportPath = Join-Path $reportDir 'metaeditor-compile.json'
New-Item -ItemType Directory -Path $reportDir -Force | Out-Null
$report = [ordered]@{
  schema = 'braintrade-v25-p0e-native-compile-v1'
  observed_utc = [DateTime]::UtcNow.ToString('o')
  source_present = (Test-Path -LiteralPath $source -PathType Leaf)
  metaeditor_compiled = 'NOT TESTED'
  bridge_connected_to_mt5 = 'NOT TESTED'
  on_tick_to_gateway = 'NOT TESTED'
  broker_execution = 'NOT TESTED'
  ready_for_real = $false
  reason = 'MetaEditor has not been run against a selected MT5 data folder'
}
function Save-Report {
  $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $reportPath -Encoding UTF8
  Write-Host ("MetaEditor report: " + $reportPath)
}
if (-not $report.source_present) {
  $report.metaeditor_compiled = 'FAIL'
  $report.reason = 'Packaged MQ5 source missing; extract complete V25 Portable'
  Save-Report
  exit 2
}
if ($InspectOnly -or [string]::IsNullOrWhiteSpace($MetaEditorPath) -or
    [string]::IsNullOrWhiteSpace($TerminalDataDir)) {
  Write-Host 'P0-E InspectOnly: MQ5 source exists. Compilation NOT TESTED.'
  Write-Host 'MT5 > File > Open Data Folder: supply that exact directory.'
  Write-Host 'Usage: .\Tools\Compile-Sniper-EA.ps1 -MetaEditorPath "C:\...\metaeditor64.exe" -TerminalDataDir "C:\...\Terminal\<MT5_DATA_FOLDER>"'
  Save-Report
  if ($InspectOnly) { exit 0 }
  exit 2
}
try {
  $editor=(Resolve-Path -LiteralPath $MetaEditorPath -ErrorAction Stop).Path
  $dataRoot=(Resolve-Path -LiteralPath $TerminalDataDir -ErrorAction Stop).Path
  if ([IO.Path]::GetFileName($editor).ToLowerInvariant() -notin @('metaeditor64.exe','metaeditor.exe')) {
    throw 'Select the official MT5 MetaEditor executable, not terminal64.exe'
  }
  if (-not (Test-Path -LiteralPath (Join-Path $dataRoot 'MQL5') -PathType Container)) {
    throw 'The selected folder must be the MT5 Data Folder containing MQL5'
  }
  $destDir=Join-Path $dataRoot 'MQL5\Experts'
  New-Item -ItemType Directory -Path $destDir -Force | Out-Null
  $dest=Join-Path $destDir 'BrainTradeSniperBridge.mq5'
  $exe=Join-Path $destDir 'BrainTradeSniperBridge.ex5'
  # Preserve local operator changes rather than silently destroying them.
  if (Test-Path -LiteralPath $dest -PathType Leaf) {
    $old=(Get-FileHash -LiteralPath $dest -Algorithm SHA256).Hash
    $new=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
    if ($old -ne $new) {
      $backup=Join-Path $destDir ("BrainTradeSniperBridge.backup-" + [DateTime]::UtcNow.ToString('yyyyMMddHHmmssfff') + '.mq5')
      Copy-Item -LiteralPath $dest -Destination $backup -ErrorAction Stop
      Write-Host ("Previous EA source saved: " + $backup)
    }
  }
  Copy-Item -LiteralPath $source -Destination $dest -Force -ErrorAction Stop
  # Use a new log for each attempt; an old successful log is not proof of a
  # fresh compile. MetaEditor is a GUI executable, so invoking it with `&`
  # does not reliably wait or populate $LASTEXITCODE in PowerShell.
  $log=Join-Path $reportDir ('metaeditor-' + [DateTime]::UtcNow.ToString('yyyyMMddHHmmssfff') + '.log')
  $previousEx5=if (Test-Path -LiteralPath $exe -PathType Leaf) { Get-Item -LiteralPath $exe } else { $null }
  $previousWrite=if ($null -ne $previousEx5) { $previousEx5.LastWriteTimeUtc } else { $null }
  $started=[DateTime]::UtcNow
  # A stale EX5 from a prior compilation is not proof of success.
  $arguments=@(('/compile:"' + $dest + '"'),('/log:"' + $log + '"'))
  $compiler=Start-Process -FilePath $editor -ArgumentList $arguments -PassThru
  if (-not $compiler.WaitForExit(120000)) {
    Stop-Process -Id $compiler.Id -Force -ErrorAction SilentlyContinue
    throw 'MetaEditor compile timed out'
  }
  $exitCode=$compiler.ExitCode
  $compiled=(Test-Path -LiteralPath $exe -PathType Leaf)
  $file=if ($compiled) { Get-Item -LiteralPath $exe } else { $null }
  $fresh=($null -ne $file -and $file.Length -gt 1024 -and
    $file.LastWriteTimeUtc -ge $started.AddSeconds(-2) -and
    ($null -eq $previousWrite -or $file.LastWriteTimeUtc -gt $previousWrite))
  $logFound=(Test-Path -LiteralPath $log -PathType Leaf)
  $logClean=$false
  if ($logFound) {
    $raw=Get-Content -LiteralPath $log -Raw -ErrorAction Stop
    $logClean=($raw -match '(?i)\b0\s+errors?\b' -and
      $raw -notmatch '(?i)\b[1-9][0-9]*\s+errors?\b')
  }
  $okay=($exitCode -eq 0 -and $fresh -and $logClean)
  $report['compiler_exit_code']=$exitCode
  $report['ex5_found']=$compiled
  $report['ex5_fresh']=$fresh
  $report['log_found']=$logFound
  $report['log_zero_errors']=$logClean
  $report.metaeditor_compiled=if ($okay) {'PASS'}else{'FAIL'}
  $report.reason=if ($okay) {
    'Fresh nonempty EX5 compiled by supplied MetaEditor; OnTick/Gateway still require MT5 Demo validation'
  }else{
    'MetaEditor failed or EX5 is stale/missing; inspect this attempt log: ' + $log
  }
  $report['ex5_sha256']=if ($okay) {(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash}else{$null}
  $report['ex5_size_bytes']=if ($null -ne $file) {$file.Length}else{$null}
  Save-Report
  if (-not $okay) { exit 1 }
  Write-Host 'MetaEditor compile PASS. Attach the generated Expert to an XAUUSD DEMO chart.'
  Write-Host 'Native/Poll delivery and MT5 Demo acceptance remain NOT TESTED on this device.'
  exit 0
} catch {
  $report.metaeditor_compiled='FAIL'
  $report.reason=[string]$_.Exception.Message
  Save-Report
  Write-Host $report.reason -ForegroundColor Red
  exit 1
}
