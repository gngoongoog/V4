[CmdletBinding()]
param(
    [ValidateSet("native", "poll", "auto")]
    [string]$Source,
    [ValidateRange(0, 86400)]
    [int]$WatchSeconds = 3600,
    [ValidateRange(1, 100000)]
    [int]$MinimumFastSamples = 200,
    [double]$PollSeconds = 1.0,
    [string]$OutputDir = "acceptance-output",
    [switch]$Reset,
    [switch]$NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-Section([string]$Text) {
    Write-Host ""
    Write-Host ("=" * 72)
    Write-Host $Text
    Write-Host ("=" * 72)
}

function Format-Ms($Value) {
    if ($null -eq $Value) { return "n/a" }
    try { return ("{0:N3} ms" -f [double]$Value) } catch { return "n/a" }
}

function Stop-WithError([string]$Message, [int]$Code = 1) {
    Write-Host "ERROR: $Message" -ForegroundColor Red
    exit $Code
}

if ($PollSeconds -lt 0.1 -or $PollSeconds -gt 60.0) {
    Stop-WithError "PollSeconds must be between 0.1 and 60.0 seconds."
}

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$PackagedPython = Join-Path $Root "runtime\python\python.exe"
$PackagedCollector = Join-Path $Root "runtime\gateway\check_runtime.py"
$SourceCollector = Join-Path $Root "gateway\check_runtime.py"

if ((Test-Path $PackagedPython) -and (Test-Path $PackagedCollector)) {
    $Python = $PackagedPython
    $Collector = $PackagedCollector
    $RuntimeKind = "portable"
} elseif (Test-Path $SourceCollector) {
    $PythonCommand = Get-Command python.exe -ErrorAction SilentlyContinue
    if ($null -eq $PythonCommand) {
        $PythonCommand = Get-Command python -ErrorAction SilentlyContinue
    }
    if ($null -eq $PythonCommand) {
        Stop-WithError "Python was not found. Run this from the packaged BrainTrade folder or install Python for source-tree diagnostics."
    }
    $Python = $PythonCommand.Source
    $Collector = $SourceCollector
    $RuntimeKind = "source"
} else {
    Stop-WithError "BrainTrade runtime collector was not found. Keep this launcher inside the BrainTrade Tools folder."
}

if (-not $PSBoundParameters.ContainsKey("Source")) {
    if ($NonInteractive) {
        $Source = "auto"
    } else {
        Write-Section "BrainTrade MT5 Demo Acceptance"
        Write-Host "This tool is READ ONLY. It never starts trading, binds an account, or sends an order."
        Write-Host "Choose the capture label for NEW Sniper orders in this session:"
        Write-Host "  1) native  - BrainTradeSniperBridge EA is attached and CONNECTED"
        Write-Host "  2) poll    - native EA is detached/stopped; 5 ms fallback polling is active"
        Write-Host "  3) auto    - do not label this session for native-vs-poll comparison"
        $Choice = Read-Host "Selection [1/2/3]"
        switch ($Choice) {
            "1" { $Source = "native" }
            "2" { $Source = "poll" }
            default { $Source = "auto" }
        }
    }
}

if ($Source -eq "native") {
    Write-Host "Capture source: NATIVE. Keep BrainTradeSniperBridge attached and CONNECTED." -ForegroundColor Cyan
} elseif ($Source -eq "poll") {
    Write-Host "Capture source: POLL. Detach/stop BrainTradeSniperBridge before collecting." -ForegroundColor Cyan
} else {
    Write-Host "Capture source: AUTO/UNLABELLED. These new samples will not be used as controlled native-vs-poll labels." -ForegroundColor Yellow
}

$ResolvedOutput = if ([System.IO.Path]::IsPathRooted($OutputDir)) {
    [System.IO.Path]::GetFullPath($OutputDir)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $Root $OutputDir))
}
New-Item -ItemType Directory -Path $ResolvedOutput -Force | Out-Null

Write-Section "Runtime preflight"
$ImportReport = Join-Path $ResolvedOutput "runtime-import-check.json"
& $Python -B $Collector $ImportReport
if ($LASTEXITCODE -ne 0) {
    Stop-WithError "Embedded Python/NumPy/MetaTrader5 import check failed. See $ImportReport" $LASTEXITCODE
}
Write-Host "Runtime imports: PASS ($RuntimeKind)"
Write-Host "Collector: $Collector"
Write-Host "Output:    $ResolvedOutput"

$CollectorArgs = @(
    $Collector,
    "--demo-acceptance",
    "--source", $Source,
    "--watch-seconds", $WatchSeconds,
    "--poll-seconds", $PollSeconds,
    "--minimum-fast-samples", $MinimumFastSamples,
    "--output-dir", $ResolvedOutput
)
if ($Reset) {
    $CollectorArgs += "--reset"
}

Write-Section "Read-only Demo capture"
Write-Host "Open BrainTradeNext.exe first and bind the currently connected MT5 account as DEMO."
Write-Host "The normal BrainTrade runtime may trade only if YOU already armed it; this launcher never arms execution."
& $Python -B @CollectorArgs
$CollectorExit = $LASTEXITCODE

$ReportPath = Join-Path $ResolvedOutput "demo-acceptance-latest.json"
if (-not (Test-Path $ReportPath)) {
    Stop-WithError "Collector did not produce $ReportPath" $(if ($CollectorExit -ne 0) { $CollectorExit } else { 1 })
}

$Report = Get-Content -Raw -Encoding UTF8 $ReportPath | ConvertFrom-Json
$Queue = $Report.fast_path.timings.queue_to_send_ms
$Send = $Report.fast_path.timings.send_ms
$Native = $Report.fast_path.sources.native
$Poll = $Report.fast_path.sources.poll

Write-Section "Acceptance summary"
Write-Host ("Result:                {0}" -f $Report.result)
$BrokerGate = if ($null -ne $Report.broker_lifecycle_acceptance) { $Report.broker_lifecycle_acceptance.status } else { "NOT TESTED" }
Write-Host ("MT5 Broker G2 cycle:    {0} (needs broker deal/history proof, not local timing)" -f $BrokerGate)
Write-Host ("Sent Sniper opens:    {0}/{1}" -f $Report.sample_counts.sent_sniper_opens, $Report.sample_counts.minimum_fast_samples)
Write-Host ("Queue-to-send P95:    {0}" -f (Format-Ms $Queue.p95))
Write-Host ("Queue-to-send P99:    {0}" -f (Format-Ms $Queue.p99))
Write-Host ("MT5 send P95:         {0}" -f (Format-Ms $Send.p95))
Write-Host ("Largest local hotspot:{0}" -f $(if ($null -eq $Report.fast_path.largest_local_pre_send_component_p95) { " n/a" } else { " " + $Report.fast_path.largest_local_pre_send_component_p95 }))
Write-Host ("Native samples:       {0} (queue P95 {1})" -f $Native.count, (Format-Ms $Native.timings.queue_to_send_ms.p95))
Write-Host ("Poll samples:         {0} (queue P95 {1})" -f $Poll.count, (Format-Ms $Poll.timings.queue_to_send_ms.p95))

if (($null -ne $Native.timings.queue_to_send_ms.p95) -and ($null -ne $Poll.timings.queue_to_send_ms.p95)) {
    $NativeP95 = [double]$Native.timings.queue_to_send_ms.p95
    $PollP95 = [double]$Poll.timings.queue_to_send_ms.p95
    $Delta = $NativeP95 - $PollP95
    if ([Math]::Abs($Delta) -lt 0.05) {
        $Faster = "tie"
    } elseif ($Delta -lt 0) {
        $Faster = "native"
    } else {
        $Faster = "poll"
    }
    Write-Host ("Native vs Poll P95:   native-poll = {0:N3} ms; faster local path = {1}" -f $Delta, $Faster) -ForegroundColor Green
}

if ($Report.warnings.Count -gt 0) {
    Write-Host ""
    Write-Host "Warnings:" -ForegroundColor Yellow
    foreach ($Warning in $Report.warnings) {
        Write-Host (" - " + $Warning)
    }
}

Write-Host ""
Write-Host "Report JSON: $ReportPath"
Write-Host ("Report CSV:  " + (Join-Path $ResolvedOutput "demo-acceptance-latest.csv"))
Write-Host "Interpret queue_to_send_ms as BrainTrade-local latency only; it is not broker fill latency or a profitability guarantee."

exit $CollectorExit
