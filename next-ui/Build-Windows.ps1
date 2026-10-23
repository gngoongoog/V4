param([string]$OriginalV4Folder = "")
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) { throw 'Install .NET 8 SDK to build the new Windows UI.' }
$Output = Join-Path $PSScriptRoot 'publish'
if (Test-Path $Output) { Remove-Item $Output -Force -Recurse }
dotnet publish .\WindowsShell\BrainTrade.Next.csproj -r win-x64 -c Release --self-contained true -p:PublishSingleFile=true -p:IncludeNativeLibrariesForSelfExtract=true -o $Output
if ($LASTEXITCODE -ne 0) { throw 'dotnet publish failed' }
Copy-Item (Join-Path $PSScriptRoot 'ui') (Join-Path $Output 'ui') -Recurse -Force
if ($OriginalV4Folder) {
    # A V4 portable folder holds the genuine BrainTrade.exe and MT5 Python runtime.
    # Copy its *contents* first, excluding any existing Next app. Do not overwrite
    # the HTML client or shell; never replace the Rust Core with a mock service.
    if (-not (Test-Path (Join-Path $OriginalV4Folder 'BrainTrade.exe'))) { throw 'Original V4 package is missing BrainTrade.exe' }
    Copy-Item (Join-Path $OriginalV4Folder '*') $Output -Recurse -Force
}
if (Test-Path (Join-Path $Output 'BrainTrade.exe')) {
    Write-Host "Ready: $Output\BrainTrade.Next.exe (Windows x64 app + authenticated V4 Rust Core)."
} else {
    Write-Warning 'UI shell compiled but Core not yet included. Copy original V4 portable contents into publish, preserving ui/ and BrainTrade.Next.exe.'
}
Write-Host 'WebView2 Runtime is required on the target Windows PC.'