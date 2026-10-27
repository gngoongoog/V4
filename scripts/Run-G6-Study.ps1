param(
  [ValidateSet('List','Register','Paper','Executed','Learning','Frozen','Admission')][string]$Action='',
  [string]$Db='',
  [string]$RunId='',
  [string]$Manifest='',
  [int]$EmbargoMinutes=30,
  [string]$Output=''
)
$ErrorActionPreference='Stop'
# The executable, Gateway and MT5 are deliberately NEVER started.
# Python G6 tools open Core SQLite via URI mode=ro with query_only enabled.
$toolDir=Join-Path $PSScriptRoot 'G6'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$python=Join-Path $root 'runtime/python/python.exe'
if(-not(Test-Path -LiteralPath $python -PathType Leaf)){
  throw 'Bundled Python missing. Keep the entire Portable folder together.'
}
if(-not $Action){
  Write-Host 'G6 Demo research, read-only. NOT trading, not independent OOS.' -ForegroundColor Yellow
  Write-Host '1 - List available Expert Frozen runs'
  Write-Host '2 - Register new prospective study (before new trades)'
  Write-Host '3 - Audit prospective paired quote-paper results'
  Write-Host '4 - Audit exact MT5 Demo booked Adaptive closes'
  Write-Host '5 - Audit causal member learning evidence, no OOS claim'
  Write-Host '6 - Check Frozen Expert policy weights against pre-freeze broker receipts'
  Write-Host '7 - Audit first local MT5 Broker learner admission time and payload hash'
  $choice=Read-Host 'Choose 1/2/3/4/5/6/7'
  $Action=switch($choice){'1'{'List'}'2'{'Register'}'3'{'Paper'}'4'{'Executed'}'5'{'Learning'}'6'{'Frozen'}'7'{'Admission'}default{throw 'Invalid selection'}}
}
if(-not $Db){$Db=Read-Host 'Enter the path to the existing Core SQLite database'}
if(-not(Test-Path -LiteralPath $Db -PathType Leaf)){throw "Core database file not found: $Db"}
$Db=(Resolve-Path -LiteralPath $Db).Path
$docs=[Environment]::GetFolderPath('MyDocuments')
$studies=Join-Path $docs 'BRAINTRADE_V25_Research'
if(-not(Test-Path $studies)){New-Item -ItemType Directory -Path $studies -Force | Out-Null}
$stamp=Get-Date -Format 'yyyyMMdd-HHmmss-ffff'
if($Action -eq 'Register'){
  Write-Host 'Find the run_id through option 1. Registration requires ACTIVE Frozen Expert.'
  if(-not $RunId){$RunId=Read-Host 'Enter exact Expert Frozen run_id'}
  if(-not $RunId){throw 'Empty run id is invalid'}
  if($EmbargoMinutes -lt 30){throw 'G6-A requires at least 30 minutes of embargo'}
  if(-not $Manifest){$Manifest=Join-Path $studies "G6A-study-$stamp.json"}
  if(Test-Path -LiteralPath $Manifest){throw 'Registration manifest already exists; refusing to overwrite it'}
  $script=Join-Path $toolDir 'audit_v25_g6a_preregistered_quotes.py'
  $arguments=@($script,'register','--db',$Db,'--run-id',$RunId,
               '--manifest',$Manifest,'--embargo-minutes',"$EmbargoMinutes")
} elseif($Action -eq 'List'){
  $script=Join-Path $toolDir 'v25_g6c_list_frozen_runs.py'
  $arguments=@($script,'--db',$Db)
} else {
  if(-not $Manifest){$Manifest=Read-Host 'Enter the existing G6-A manifest JSON path'}
  if(-not(Test-Path -LiteralPath $Manifest -PathType Leaf)){throw 'Existing study manifest is required'}
  $Manifest=(Resolve-Path -LiteralPath $Manifest).Path
  if(-not $Output){
    $prefix=switch($Action){'Paper'{'G6A-paper'}'Executed'{'G6B-booked-Demo'}'Learning'{'G6D-learning-evidence'}'Frozen'{'G6E-frozen-cutoff'}'Admission'{'G6G-admission-clock'}}
    $Output=Join-Path $studies "$prefix-$stamp.json"
  }
  if(Test-Path -LiteralPath $Output){throw 'Report output already exists; refusing to overwrite it'}
  if($Action -eq 'Paper'){
    $script=Join-Path $toolDir 'audit_v25_g6a_preregistered_quotes.py'
    $arguments=@($script,'audit','--db',$Db,'--manifest',$Manifest,'--output',$Output)
  } elseif($Action -eq 'Executed'){
    $script=Join-Path $toolDir 'audit_v25_g6b_executed_demo_arm.py'
    $arguments=@($script,'--db',$Db,'--manifest',$Manifest,'--output',$Output)
  } elseif($Action -eq 'Learning'){
    $script=Join-Path $toolDir 'audit_v25_g6d_causal_learning_evidence.py'
    $arguments=@($script,'--db',$Db,'--manifest',$Manifest,'--output',$Output)
  } elseif($Action -eq 'Frozen'){
    $script=Join-Path $toolDir 'audit_v25_g6e_frozen_policy_cutoff.py'
    $arguments=@($script,'--db',$Db,'--manifest',$Manifest,'--output',$Output)
  } else {
    $script=Join-Path $toolDir 'audit_v25_g6g_broker_admission_clock.py'
    $arguments=@($script,'--db',$Db,'--manifest',$Manifest,'--output',$Output)
  }
}
if(-not(Test-Path -LiteralPath $script -PathType Leaf)){
  throw "Missing G6 study tool: $script"
}
Write-Host "Running read-only: $Action" -ForegroundColor Cyan
& $python @arguments
$exitCode=$LASTEXITCODE
if($exitCode -ne 0){throw "Study validation failed (code $exitCode). No result is certified."}
Write-Host 'Study completed; any gains remain research estimates, never REAL readiness.' -ForegroundColor Green
if($Action -ne 'List'){
  $path=if($Action -eq 'Register'){$Manifest}else{$Output}
  Write-Host "Output: $path"
}
