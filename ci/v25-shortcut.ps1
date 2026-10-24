param([switch]$Remove)
$ErrorActionPreference='Stop'
try {
    $folder=Split-Path -Parent $MyInvocation.MyCommand.Path
    $target=Join-Path $folder 'BrainTradeNext.exe'
    if(-not $Remove -and -not (Test-Path -LiteralPath $target -PathType Leaf)) {
        throw 'Extract the COMPLETE portable folder before creating a shortcut.'
    }
    $desktop=[Environment]::GetFolderPath('DesktopDirectory')
    $programs=[Environment]::GetFolderPath('Programs')
    if(-not $desktop -or -not $programs) { throw 'No Windows desktop/start menu location' }
    $startFolder=Join-Path $programs 'BRAINTRADE V4 Portable'
    $shell=New-Object -ComObject WScript.Shell
    $links=@((Join-Path $desktop 'BRAINTRADE V4.lnk'),(Join-Path $startFolder 'BRAINTRADE V4.lnk'))
    if($Remove) {
        foreach($link in $links) {
            if(Test-Path -LiteralPath $link -PathType Leaf) {
                $entry=$shell.CreateShortcut($link)
                if($entry.TargetPath -and $entry.TargetPath.Equals($target,[StringComparison]::OrdinalIgnoreCase)){
                    Remove-Item -LiteralPath $link -Force
                }
            }
        }
        if(Test-Path $startFolder -PathType Container) {
            if(@(Get-ChildItem -LiteralPath $startFolder -Force).Count -eq 0) {
                Remove-Item -LiteralPath $startFolder
            }
        }
    } else {
        New-Item -ItemType Directory -Path $startFolder -Force|Out-Null
        foreach($link in $links){
            if(Test-Path -LiteralPath $link -PathType Leaf){
                $existing=$shell.CreateShortcut($link)
                if($existing.TargetPath -and -not $existing.TargetPath.Equals($target,[StringComparison]::OrdinalIgnoreCase)){
                    throw ('Refusing to overwrite another installation shortcut: '+$link)
                }
            }
            $entry=$shell.CreateShortcut($link)
            $entry.TargetPath=$target
            $entry.WorkingDirectory=$folder
            $entry.IconLocation=$target+',0'
            $entry.Description='BRAINTRADE V4 Enterprise V25 portable'
            $entry.Save()
        }
    }
} catch { Write-Error $_; exit 1 }
