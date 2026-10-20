[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.Windows.Forms.Application]::EnableVisualStyles()

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$CliScript = Join-Path $PSScriptRoot "Run-Demo-Acceptance.ps1"
if (-not (Test-Path $CliScript)) {
    [System.Windows.Forms.MessageBox]::Show(
        "Run-Demo-Acceptance.ps1 was not found next to this GUI launcher.",
        "BrainTrade Demo Acceptance",
        [System.Windows.Forms.MessageBoxButtons]::OK,
        [System.Windows.Forms.MessageBoxIcon]::Error
    ) | Out-Null
    exit 1
}

$OutputDir = Join-Path $Root "acceptance-output"
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
$ReportPath = Join-Path $OutputDir "demo-acceptance-latest.json"
$StdoutPath = Join-Path $OutputDir "acceptance-gui-stdout.log"
$StderrPath = Join-Path $OutputDir "acceptance-gui-stderr.log"

function New-Label {
    param([string]$Text, [int]$X, [int]$Y, [int]$W, [int]$H, [float]$Size = 9, [bool]$Bold = $false)
    $Label = New-Object System.Windows.Forms.Label
    $Label.Text = $Text
    $Label.Location = New-Object System.Drawing.Point($X, $Y)
    $Label.Size = New-Object System.Drawing.Size($W, $H)
    $Label.ForeColor = [System.Drawing.Color]::FromArgb(220, 228, 240)
    $Style = if ($Bold) { [System.Drawing.FontStyle]::Bold } else { [System.Drawing.FontStyle]::Regular }
    $Label.Font = New-Object System.Drawing.Font("Segoe UI", $Size, $Style)
    return $Label
}

function Set-Card {
    param([System.Windows.Forms.Label]$Label, $Value)
    if ($null -eq $Value -or "$Value" -eq "") {
        $Label.Text = "—"
    } else {
        $Label.Text = "$Value"
    }
}

function Format-MsValue($Value) {
    if ($null -eq $Value) { return "—" }
    try { return ("{0:N3} ms" -f [double]$Value) } catch { return "—" }
}

function Read-AcceptanceReport {
    if (-not (Test-Path $ReportPath)) { return $null }
    return Get-Content -Raw -Encoding UTF8 $ReportPath | ConvertFrom-Json
}

function Load-AcceptanceReport {
    if (-not (Test-Path $ReportPath)) {
        $StatusValue.Text = "NO REPORT"
        $StatusValue.ForeColor = [System.Drawing.Color]::FromArgb(240, 197, 120)
        $SummaryBox.Text = "لا يوجد تقرير بعد. شغّل جلسة Native أو Poll أولاً."
        return
    }
    try {
        $Report = Read-AcceptanceReport
        $Result = [string]$Report.result
        $StatusValue.Text = $Result
        $StatusValue.ForeColor = if ($Result -eq "TARGET_MET") {
            [System.Drawing.Color]::FromArgb(91, 224, 190)
        } elseif ($Result -eq "CONTRACT_FAIL") {
            [System.Drawing.Color]::FromArgb(242, 128, 128)
        } else {
            [System.Drawing.Color]::FromArgb(240, 197, 120)
        }

        Set-Card $SamplesValue $Report.sample_counts.sent_sniper_opens
        $Queue95Value.Text = Format-MsValue $Report.fast_path.timings.queue_to_send_ms.p95
        $Queue99Value.Text = Format-MsValue $Report.fast_path.timings.queue_to_send_ms.p99
        $Send95Value.Text = Format-MsValue $Report.fast_path.timings.send_ms.p95
        Set-Card $NativeCountValue $Report.fast_path.sources.native.count
        Set-Card $PollCountValue $Report.fast_path.sources.poll.count
        $Native95Value.Text = Format-MsValue $Report.fast_path.sources.native.timings.queue_to_send_ms.p95
        $Poll95Value.Text = Format-MsValue $Report.fast_path.sources.poll.timings.queue_to_send_ms.p95

        $Hotspot = $Report.fast_path.largest_local_pre_send_component_p95
        $Warnings = @($Report.warnings)
        $Lines = New-Object System.Collections.Generic.List[string]
        $Lines.Add("النتيجة: $Result")
        $Lines.Add("أكبر مكوّن محلي قبل الإرسال: " + $(if ($null -eq $Hotspot) { "غير متاح" } else { [string]$Hotspot }))
        if (($null -ne $Report.fast_path.sources.native.timings.queue_to_send_ms.p95) -and ($null -ne $Report.fast_path.sources.poll.timings.queue_to_send_ms.p95)) {
            $NativeP95 = [double]$Report.fast_path.sources.native.timings.queue_to_send_ms.p95
            $PollP95 = [double]$Report.fast_path.sources.poll.timings.queue_to_send_ms.p95
            $Delta = $NativeP95 - $PollP95
            $Faster = if ([Math]::Abs($Delta) -lt 0.05) { "متعادل" } elseif ($Delta -lt 0) { "Native" } else { "Poll" }
            $Lines.Add(("Native - Poll P95 = {0:N3} ms | الأسرع محلياً: {1}" -f $Delta, $Faster))
        }
        if ($Warnings.Count -gt 0) {
            $Lines.Add("")
            $Lines.Add("تحذيرات:")
            foreach ($Warning in $Warnings) { $Lines.Add("• " + [string]$Warning) }
        }
        $Lines.Add("")
        $Lines.Add("queue_to_send_ms يقيس زمن BrainTrade المحلي فقط، وليس زمن تنفيذ/Fill الوسيط ولا ضمان ربح.")
        $SummaryBox.Text = $Lines -join [Environment]::NewLine
    } catch {
        $StatusValue.Text = "REPORT ERROR"
        $StatusValue.ForeColor = [System.Drawing.Color]::FromArgb(242, 128, 128)
        $SummaryBox.Text = "تعذر قراءة التقرير: $($_.Exception.Message)"
    }
}

function Export-ShareBundle {
    if (-not (Test-Path $ReportPath)) {
        [System.Windows.Forms.MessageBox]::Show(
            "لا يوجد تقرير لتصديره بعد.",
            "BrainTrade Demo Acceptance",
            [System.Windows.Forms.MessageBoxButtons]::OK,
            [System.Windows.Forms.MessageBoxIcon]::Information
        ) | Out-Null
        return
    }

    $Staging = Join-Path $OutputDir ("share-staging-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $Staging -Force | Out-Null
    try {
        $AllowedFiles = @(
            "demo-acceptance-latest.json",
            "demo-acceptance-latest.csv",
            "demo-acceptance-state.json",
            "runtime-import-check.json"
        )
        foreach ($Name in $AllowedFiles) {
            $Source = Join-Path $OutputDir $Name
            if (Test-Path $Source) {
                Copy-Item -LiteralPath $Source -Destination (Join-Path $Staging $Name) -Force
            }
        }

        $Report = Read-AcceptanceReport
        $ShareSummary = [ordered]@{
            schema = 1
            generated_utc = [DateTime]::UtcNow.ToString("o")
            privacy = "No raw MT5 login, ipc.key, broker journal/database, Telegram credentials, or GUI stdout/stderr logs are included by this exporter."
            result = $Report.result
            version = $Report.metadata.version
            account_fingerprint = $Report.metadata.account_fingerprint
            symbol = $Report.metadata.symbol
            currency = $Report.metadata.currency
            target = $Report.metadata.target
            trading_mode_at_start = $Report.metadata.trading_mode_at_start
            capture_source = $Report.metadata.capture_source
            sample_counts = $Report.sample_counts
            broker_result_states = $Report.broker_result_states
            broker_retcodes = $Report.broker_retcodes
            queue_to_send_ms = $Report.fast_path.timings.queue_to_send_ms
            send_ms = $Report.fast_path.timings.send_ms
            native = $Report.fast_path.sources.native
            poll = $Report.fast_path.sources.poll
            largest_local_pre_send_component_p95 = $Report.fast_path.largest_local_pre_send_component_p95
            contracts = $Report.contracts
            warnings = $Report.warnings
        }
        $SummaryPath = Join-Path $Staging "share-summary.json"
        $ShareSummary | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $SummaryPath -Encoding UTF8

        $Readme = @(
            "BrainTrade Demo Acceptance share bundle",
            "",
            "This bundle is intended for diagnosis of the target-machine Demo acceptance run.",
            "It does not contain the raw MT5 login, ipc.key, broker databases/journals, Telegram credentials, or GUI stdout/stderr logs.",
            "The account_fingerprint is a short one-way fingerprint used only to keep samples from different accounts separate.",
            "queue_to_send_ms is BrainTrade-local timing; it is not broker fill latency or a profitability guarantee."
        ) -join [Environment]::NewLine
        Set-Content -LiteralPath (Join-Path $Staging "SHARE_README.txt") -Value $Readme -Encoding UTF8

        $Timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
        $ZipPath = Join-Path $OutputDir ("BrainTrade-Demo-Acceptance-$Timestamp.zip")
        if (Test-Path $ZipPath) { Remove-Item -LiteralPath $ZipPath -Force }
        [System.IO.Compression.ZipFile]::CreateFromDirectory($Staging, $ZipPath, [System.IO.Compression.CompressionLevel]::Optimal, $false)

        $ProgressLabel.Text = "تم إنشاء حزمة مشاركة آمنة: $ZipPath"
        [System.Windows.Forms.MessageBox]::Show(
            "تم إنشاء حزمة النتائج القابلة للمشاركة:`n$ZipPath`n`nلا تحتوي Login خام أو ipc.key أو قواعد بيانات التداول أو سجلات GUI.",
            "BrainTrade Demo Acceptance",
            [System.Windows.Forms.MessageBoxButtons]::OK,
            [System.Windows.Forms.MessageBoxIcon]::Information
        ) | Out-Null
    } catch {
        [System.Windows.Forms.MessageBox]::Show(
            "تعذر إنشاء حزمة المشاركة: $($_.Exception.Message)",
            "BrainTrade Demo Acceptance",
            [System.Windows.Forms.MessageBoxButtons]::OK,
            [System.Windows.Forms.MessageBoxIcon]::Error
        ) | Out-Null
    } finally {
        if (Test-Path $Staging) { Remove-Item -LiteralPath $Staging -Recurse -Force -ErrorAction SilentlyContinue }
    }
}

$Form = New-Object System.Windows.Forms.Form
$Form.Text = "BrainTrade — Demo Acceptance Center"
$Form.StartPosition = "CenterScreen"
$Form.Size = New-Object System.Drawing.Size(1040, 800)
$Form.MinimumSize = New-Object System.Drawing.Size(940, 720)
$Form.BackColor = [System.Drawing.Color]::FromArgb(11, 18, 32)
$Form.ForeColor = [System.Drawing.Color]::FromArgb(235, 240, 248)
$Form.Font = New-Object System.Drawing.Font("Segoe UI", 9)

$Title = New-Label "BRAINTRADE — DEMO ACCEPTANCE CENTER" 24 18 720 34 17 $true
$Form.Controls.Add($Title)
$ReadOnly = New-Label "READ ONLY • لا تشغّل التداول، لا تغيّر START/STOP، ولا ترسل أوامر للوسيط" 24 55 900 28 10 $true
$ReadOnly.ForeColor = [System.Drawing.Color]::FromArgb(91, 224, 190)
$Form.Controls.Add($ReadOnly)

$SettingsPanel = New-Object System.Windows.Forms.Panel
$SettingsPanel.Location = New-Object System.Drawing.Point(24, 92)
$SettingsPanel.Size = New-Object System.Drawing.Size(974, 92)
$SettingsPanel.BackColor = [System.Drawing.Color]::FromArgb(17, 26, 40)
$Form.Controls.Add($SettingsPanel)

$SettingsPanel.Controls.Add((New-Label "المسار" 14 10 70 24 9 $true))
$SourceCombo = New-Object System.Windows.Forms.ComboBox
$SourceCombo.Location = New-Object System.Drawing.Point(14, 36)
$SourceCombo.Size = New-Object System.Drawing.Size(210, 28)
$SourceCombo.DropDownStyle = [System.Windows.Forms.ComboBoxStyle]::DropDownList
[void]$SourceCombo.Items.Add("Native — EA CONNECTED")
[void]$SourceCombo.Items.Add("Poll — EA detached/stopped")
[void]$SourceCombo.Items.Add("Auto — unlabelled")
$SourceCombo.SelectedIndex = 0
$SettingsPanel.Controls.Add($SourceCombo)

$SettingsPanel.Controls.Add((New-Label "مدة القياس" 245 10 100 24 9 $true))
$DurationCombo = New-Object System.Windows.Forms.ComboBox
$DurationCombo.Location = New-Object System.Drawing.Point(245, 36)
$DurationCombo.Size = New-Object System.Drawing.Size(140, 28)
$DurationCombo.DropDownStyle = [System.Windows.Forms.ComboBoxStyle]::DropDownList
[void]$DurationCombo.Items.Add("5 دقائق")
[void]$DurationCombo.Items.Add("15 دقيقة")
[void]$DurationCombo.Items.Add("30 دقيقة")
[void]$DurationCombo.Items.Add("60 دقيقة")
$DurationCombo.SelectedIndex = 1
$SettingsPanel.Controls.Add($DurationCombo)

$SettingsPanel.Controls.Add((New-Label "أقل عينات Sniper" 405 10 125 24 9 $true))
$MinSamples = New-Object System.Windows.Forms.NumericUpDown
$MinSamples.Location = New-Object System.Drawing.Point(405, 36)
$MinSamples.Size = New-Object System.Drawing.Size(120, 28)
$MinSamples.Minimum = 1
$MinSamples.Maximum = 100000
$MinSamples.Value = 200
$SettingsPanel.Controls.Add($MinSamples)

$ResetCheck = New-Object System.Windows.Forms.CheckBox
$ResetCheck.Text = "بدء عينة جديدة / Reset"
$ResetCheck.Location = New-Object System.Drawing.Point(548, 37)
$ResetCheck.Size = New-Object System.Drawing.Size(190, 26)
$ResetCheck.ForeColor = [System.Drawing.Color]::FromArgb(205, 217, 235)
$SettingsPanel.Controls.Add($ResetCheck)

$RunButton = New-Object System.Windows.Forms.Button
$RunButton.Text = "ابدأ القياس"
$RunButton.Location = New-Object System.Drawing.Point(758, 30)
$RunButton.Size = New-Object System.Drawing.Size(190, 38)
$SettingsPanel.Controls.Add($RunButton)

$CardsPanel = New-Object System.Windows.Forms.Panel
$CardsPanel.Location = New-Object System.Drawing.Point(24, 200)
$CardsPanel.Size = New-Object System.Drawing.Size(974, 166)
$CardsPanel.BackColor = [System.Drawing.Color]::FromArgb(17, 26, 40)
$Form.Controls.Add($CardsPanel)

function Add-Card {
    param([string]$Caption, [int]$X, [int]$Y, [int]$W = 180)
    $CardsPanel.Controls.Add((New-Label $Caption $X $Y $W 22 8 $false))
    $Value = New-Label "—" $X ($Y + 24) $W 34 15 $true
    $Value.ForeColor = [System.Drawing.Color]::FromArgb(241, 245, 252)
    $CardsPanel.Controls.Add($Value)
    return $Value
}

$StatusValue = Add-Card "RESULT" 16 12 210
$SamplesValue = Add-Card "SNIPER SAMPLES" 246 12 160
$Queue95Value = Add-Card "QUEUE→SEND P95" 426 12 170
$Queue99Value = Add-Card "QUEUE→SEND P99" 616 12 170
$Send95Value = Add-Card "MT5 SEND P95" 806 12 150
$NativeCountValue = Add-Card "NATIVE COUNT" 16 92 140
$Native95Value = Add-Card "NATIVE P95" 176 92 170
$PollCountValue = Add-Card "POLL COUNT" 366 92 140
$Poll95Value = Add-Card "POLL P95" 526 92 170

$ExportButton = New-Object System.Windows.Forms.Button
$ExportButton.Text = "تصدير حزمة مشاركة"
$ExportButton.Location = New-Object System.Drawing.Point(528, 378)
$ExportButton.Size = New-Object System.Drawing.Size(180, 34)
$Form.Controls.Add($ExportButton)

$LoadButton = New-Object System.Windows.Forms.Button
$LoadButton.Text = "تحديث التقرير"
$LoadButton.Location = New-Object System.Drawing.Point(718, 378)
$LoadButton.Size = New-Object System.Drawing.Size(120, 34)
$Form.Controls.Add($LoadButton)

$OpenButton = New-Object System.Windows.Forms.Button
$OpenButton.Text = "فتح مجلد النتائج"
$OpenButton.Location = New-Object System.Drawing.Point(848, 378)
$OpenButton.Size = New-Object System.Drawing.Size(150, 34)
$Form.Controls.Add($OpenButton)

$SummaryLabel = New-Label "التشخيص" 24 424 160 24 10 $true
$Form.Controls.Add($SummaryLabel)
$SummaryBox = New-Object System.Windows.Forms.TextBox
$SummaryBox.Location = New-Object System.Drawing.Point(24, 452)
$SummaryBox.Size = New-Object System.Drawing.Size(974, 166)
$SummaryBox.Multiline = $true
$SummaryBox.ReadOnly = $true
$SummaryBox.ScrollBars = [System.Windows.Forms.ScrollBars]::Vertical
$SummaryBox.BackColor = [System.Drawing.Color]::FromArgb(21, 31, 48)
$SummaryBox.ForeColor = [System.Drawing.Color]::FromArgb(205, 217, 235)
$SummaryBox.BorderStyle = [System.Windows.Forms.BorderStyle]::FixedSingle
$SummaryBox.Font = New-Object System.Drawing.Font("Segoe UI", 9.5)
$Form.Controls.Add($SummaryBox)

$ProgressLabel = New-Label "جاهز" 24 630 940 26 9 $true
$ProgressLabel.ForeColor = [System.Drawing.Color]::FromArgb(141, 156, 180)
$Form.Controls.Add($ProgressLabel)
$LogBox = New-Object System.Windows.Forms.TextBox
$LogBox.Location = New-Object System.Drawing.Point(24, 660)
$LogBox.Size = New-Object System.Drawing.Size(974, 70)
$LogBox.Multiline = $true
$LogBox.ReadOnly = $true
$LogBox.ScrollBars = [System.Windows.Forms.ScrollBars]::Vertical
$LogBox.BackColor = [System.Drawing.Color]::FromArgb(10, 15, 24)
$LogBox.ForeColor = [System.Drawing.Color]::FromArgb(141, 156, 180)
$LogBox.BorderStyle = [System.Windows.Forms.BorderStyle]::FixedSingle
$Form.Controls.Add($LogBox)

$script:AcceptanceProcess = $null
$script:StartedAt = $null
$Timer = New-Object System.Windows.Forms.Timer
$Timer.Interval = 500

$Timer.Add_Tick({
    if ($null -eq $script:AcceptanceProcess) { return }
    try {
        if (-not $script:AcceptanceProcess.HasExited) {
            $Elapsed = [int]((Get-Date) - $script:StartedAt).TotalSeconds
            $ProgressLabel.Text = "قياس قيد التشغيل… ${Elapsed}s — يمكنك ترك BrainTrade يعمل بصورة طبيعية. هذه النافذة لا تفعّل التداول."
            if (Test-Path $StdoutPath) {
                $Tail = @(Get-Content -Path $StdoutPath -Tail 3 -ErrorAction SilentlyContinue)
                if ($Tail.Count -gt 0) { $LogBox.Text = $Tail -join [Environment]::NewLine }
            }
            return
        }

        $ExitCode = $script:AcceptanceProcess.ExitCode
        $Timer.Stop()
        $RunButton.Enabled = $true
        $ExportButton.Enabled = $true
        $SourceCombo.Enabled = $true
        $DurationCombo.Enabled = $true
        $MinSamples.Enabled = $true
        $ResetCheck.Enabled = $true
        $ProgressLabel.Text = "انتهى القياس — Exit code $ExitCode"
        if (Test-Path $StdoutPath) {
            $Tail = @(Get-Content -Path $StdoutPath -Tail 8 -ErrorAction SilentlyContinue)
            $LogBox.Text = $Tail -join [Environment]::NewLine
        }
        if (($ExitCode -ne 0) -and (Test-Path $StderrPath)) {
            $ErrorTail = @(Get-Content -Path $StderrPath -Tail 5 -ErrorAction SilentlyContinue)
            if ($ErrorTail.Count -gt 0) {
                $LogBox.Text += [Environment]::NewLine + ($ErrorTail -join [Environment]::NewLine)
            }
        }
        Load-AcceptanceReport
        $script:AcceptanceProcess.Dispose()
        $script:AcceptanceProcess = $null
    } catch {
        $Timer.Stop()
        $RunButton.Enabled = $true
        $ExportButton.Enabled = $true
        $ProgressLabel.Text = "خطأ أثناء مراقبة جلسة القياس: $($_.Exception.Message)"
        $script:AcceptanceProcess = $null
    }
})

$RunButton.Add_Click({
    if ($null -ne $script:AcceptanceProcess) { return }
    $Source = @("native", "poll", "auto")[$SourceCombo.SelectedIndex]
    $WatchSeconds = @(300, 900, 1800, 3600)[$DurationCombo.SelectedIndex]
    $Minimum = [int]$MinSamples.Value

    if ($Source -eq "native") {
        $Confirm = [System.Windows.Forms.MessageBox]::Show(
            "تأكد أن BrainTradeSniperBridge مربوط على الشارت ويعرض CONNECTED.`n`nالأداة ستقرأ القياسات فقط ولن تشغّل التداول.",
            "Native session",
            [System.Windows.Forms.MessageBoxButtons]::OKCancel,
            [System.Windows.Forms.MessageBoxIcon]::Information
        )
        if ($Confirm -ne [System.Windows.Forms.DialogResult]::OK) { return }
    } elseif ($Source -eq "poll") {
        $Confirm = [System.Windows.Forms.MessageBox]::Show(
            "افصل/أوقف BrainTradeSniperBridge قبل جلسة Poll حتى تكون المقارنة مضبوطة.`n`nالأداة ستقرأ القياسات فقط ولن تشغّل التداول.",
            "Poll session",
            [System.Windows.Forms.MessageBoxButtons]::OKCancel,
            [System.Windows.Forms.MessageBoxIcon]::Information
        )
        if ($Confirm -ne [System.Windows.Forms.DialogResult]::OK) { return }
    }

    Remove-Item $StdoutPath, $StderrPath -Force -ErrorAction SilentlyContinue
    $Args = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", ('"' + $CliScript + '"'),
        "-Source", $Source,
        "-WatchSeconds", [string]$WatchSeconds,
        "-MinimumFastSamples", [string]$Minimum,
        "-NonInteractive"
    )
    if ($ResetCheck.Checked) { $Args += "-Reset" }

    try {
        $script:AcceptanceProcess = Start-Process -FilePath "powershell.exe" -ArgumentList $Args -WindowStyle Hidden -RedirectStandardOutput $StdoutPath -RedirectStandardError $StderrPath -PassThru
        $script:StartedAt = Get-Date
        $RunButton.Enabled = $false
        $ExportButton.Enabled = $false
        $SourceCombo.Enabled = $false
        $DurationCombo.Enabled = $false
        $MinSamples.Enabled = $false
        $ResetCheck.Enabled = $false
        $ProgressLabel.Text = "بدأت جلسة $Source لمدة $WatchSeconds ثانية…"
        $LogBox.Text = "تشغيل collector القراءة فقط…"
        $Timer.Start()
    } catch {
        $script:AcceptanceProcess = $null
        $ProgressLabel.Text = "تعذر بدء القياس: $($_.Exception.Message)"
    }
})

$ExportButton.Add_Click({ Export-ShareBundle })
$LoadButton.Add_Click({ Load-AcceptanceReport })
$OpenButton.Add_Click({
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
    Start-Process -FilePath "explorer.exe" -ArgumentList ('"' + $OutputDir + '"')
})

$Form.Add_FormClosing({
    if (($null -ne $script:AcceptanceProcess) -and (-not $script:AcceptanceProcess.HasExited)) {
        $Choice = [System.Windows.Forms.MessageBox]::Show(
            "جلسة القياس ما زالت تعمل. إغلاق النافذة سيوقف collector فقط ولا يغيّر حالة تداول BrainTrade. هل تريد الإغلاق؟",
            "BrainTrade Demo Acceptance",
            [System.Windows.Forms.MessageBoxButtons]::YesNo,
            [System.Windows.Forms.MessageBoxIcon]::Warning
        )
        if ($Choice -ne [System.Windows.Forms.DialogResult]::Yes) {
            $_.Cancel = $true
            return
        }
        try {
            Start-Process -FilePath "taskkill.exe" -ArgumentList @("/PID", [string]$script:AcceptanceProcess.Id, "/T", "/F") -WindowStyle Hidden -Wait | Out-Null
        } catch {
            try { $script:AcceptanceProcess.Kill() } catch {}
        }
    }
})

Load-AcceptanceReport
[void]$Form.ShowDialog()
