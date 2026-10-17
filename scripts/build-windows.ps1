param([switch]$SkipTests)
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed ($LASTEXITCODE)" }
}
function Test-PowerShellSyntax {
    param([string]$Path)
    $Tokens = $null
    $ParserErrors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile($Path, [ref]$Tokens, [ref]$ParserErrors)
    if ($ParserErrors.Count -gt 0) {
        $Details = ($ParserErrors | ForEach-Object { "$($_.Extent.StartLineNumber):$($_.Extent.StartColumnNumber) $($_.Message)" }) -join '; '
        throw "PowerShell syntax validation failed for $Path`: $Details"
    }
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'Build machine requires Rust stable and Visual Studio C++ Build Tools.' }
if (-not (Get-Command py -ErrorAction SilentlyContinue)) { throw 'Build machine requires Python 3.13 x64.' }
Test-PowerShellSyntax -Path (Join-Path $PWD 'scripts\Run-Demo-Acceptance.ps1')
Invoke-Checked -Program py -Arguments @('-3.13','-m','venv','.build-venv')
$Python = Join-Path $PWD '.build-venv\Scripts\python.exe'
Invoke-Checked -Program $Python -Arguments @('-m','pip','install','pefile==2024.8.26')
if (-not $SkipTests) {
    Invoke-Checked -Program cargo -Arguments @('test','--workspace','--locked')
    Invoke-Checked -Program $Python -Arguments @('-m','unittest','discover','-s','tests','-v')
}
Invoke-Checked -Program cargo -Arguments @('build','--release','--locked','-p','braintrade-desktop')
Invoke-Checked -Program $Python -Arguments @('scripts\package-portable.py','--exe','target\release\BrainTrade.exe')
$PackagedOperator = Join-Path $PWD 'release\BrainTrade\Tools\Run-Demo-Acceptance.ps1'
Test-PowerShellSyntax -Path $PackagedOperator
$EmbeddedPython = Join-Path $PWD 'release\BrainTrade\runtime\python\python.exe'
$Check = Join-Path $PWD 'release\BrainTrade\runtime\gateway\check_runtime.py'
Invoke-Checked -Program $EmbeddedPython -Arguments @($Check)
Invoke-Checked -Program $Python -Arguments @('scripts\check_windows_portable.py','--exe','release\BrainTrade\BrainTrade.exe')
Invoke-Checked -Program $Python -Arguments @('scripts\check_execution.py','--exe','release\BrainTrade\BrainTrade.exe')
Write-Host 'Portable package created. End users launch BrainTrade.exe; no separate Python/Rust installation.'
Write-Host 'Demo acceptance launcher syntax was validated on Windows, but real MT5 Demo acceptance remains a target-machine step.'
