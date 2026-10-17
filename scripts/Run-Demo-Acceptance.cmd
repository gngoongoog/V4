@echo off
setlocal
set "SCRIPT_DIR=%~dp0"

if exist "%SCRIPT_DIR%Tools\Run-Demo-Acceptance.ps1" (
  set "PS_SCRIPT=%SCRIPT_DIR%Tools\Run-Demo-Acceptance.ps1"
) else (
  set "PS_SCRIPT=%SCRIPT_DIR%Run-Demo-Acceptance.ps1"
)

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%PS_SCRIPT%" %*
set "EXIT_CODE=%ERRORLEVEL%"

echo.
if not "%EXIT_CODE%"=="0" echo BrainTrade Demo Acceptance exited with code %EXIT_CODE%.
pause
exit /b %EXIT_CODE%
