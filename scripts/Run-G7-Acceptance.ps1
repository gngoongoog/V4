# G7-H read-only acceptance from the shipped V25 Portable on Windows.
# Never calls MT5, START/STOP, modifies Core SQLite, or transmits credentials.
[CmdletBinding()]
param(
  [string]$Db = "",
  [string]$Before = "",
  [string]$Manifest = "",
  [string]$Output = "",
  [switch]$NoDialog
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Python = Join-Path $Root 'runtime\python\python.exe'
$Collector = Join-Path $PSScriptRoot 'G7\g7_demo_acceptance_packet.py'
if (-not (Test-Path -LiteralPath $Python -PathType Leaf)) { throw 'Embedded Python is missing; extract the COMPLETE V25 ZIP.' }
if (-not (Test-Path -LiteralPath $Collector -PathType Leaf)) { throw 'G7-H research collector is missing; use a complete V25 ZIP.' }
if (-not $Db -and $env:BRAINTRADE_DATA_DIR) {
  $Suggested = Join-Path $env:BRAINTRADE_DATA_DIR 'events.sqlite'
  if (Test-Path -LiteralPath $Suggested -PathType Leaf) { $Db = $Suggested }
}
if (-not $Db -and -not $NoDialog) {
  Add-Type -AssemblyName System.Windows.Forms
  $Picker = New-Object System.Windows.Forms.OpenFileDialog
  $Picker.Title = 'اختر قاعدة Core الفعلية events.sqlite (القراءة فقط)'
  $Picker.Filter = 'Core events.sqlite|events.sqlite|SQLite database|*.sqlite|All files|*.*'
  $Picker.CheckFileExists = $true
  if ($Picker.ShowDialog() -ne [System.Windows.Forms.DialogResult]::OK) {
    Write-Host 'No database was selected. No changes made.'
    exit 2
  }
  $Db = $Picker.FileName
}
if (-not $Db -or -not (Test-Path -LiteralPath $Db -PathType Leaf)) {
  throw 'Core events.sqlite not found; select the actual Core database or pass -Db PATH.'
}
if (-not $Output) {
  $Docs = [Environment]::GetFolderPath('MyDocuments')
  if (-not $Docs) { $Docs = $env:USERPROFILE }
  $Dir = Join-Path $Docs 'BRAINTRADE_V25_G7_Reports'
  New-Item -ItemType Directory -Path $Dir -Force | Out-Null
  $Output = Join-Path $Dir ('G7_H_Acceptance_' + (Get-Date -Format 'yyyyMMdd_HHmmss_fff') + '.json')
}
if (([IO.Path]::GetFullPath($Output) -eq [IO.Path]::GetFullPath($Db)) -or (Test-Path -LiteralPath $Output)) {
  throw 'The report must be a NEW file, never overwrite Core SQLite or an existing report.'
}
foreach ($Extra in @($Before, $Manifest)) {
  if ($Extra -and -not (Test-Path -LiteralPath $Extra -PathType Leaf)) { throw "Optional source not found: $Extra" }
}
$Arguments = @('-B', $Collector, '--db', $Db, '--output', $Output)
if ($Before) { $Arguments += @('--before', $Before) }
if ($Manifest) { $Arguments += @('--manifest', $Manifest) }
$env:PYTHONDONTWRITEBYTECODE = '1'
Write-Host 'G7-H: auditing READ-ONLY Core evidence. NO broker orders and NO account permissions.'
& $Python @Arguments
if ($LASTEXITCODE -ne 0) { throw "G7-H integrity audit failed ($LASTEXITCODE). The source is unchanged." }
Write-Host "G7-H report: $Output"
Write-Host 'This is local evidence only, not successful broker validation or LIVE approval.'
