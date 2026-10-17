@echo off
setlocal
set "SCRIPT_DIR=%~dp0"

if exist "%SCRIPT_DIR%Tools\Run-Demo-Acceptance.ps1" (
  set "CLI_SCRIPT=%SCRIPT_DIR%Tools\Run-Demo-Acceptance.ps1"
  set "GUI_SCRIPT=%SCRIPT_DIR%Tools\Run-Demo-Acceptance-GUI.ps1"
) else (
  set "CLI_SCRIPT=%SCRIPT_DIR%Run-Demo-Acceptance.ps1"
  set "GUI_SCRIPT=%SCRIPT_DIR%Run-Demo-Acceptance-GUI.ps1"
)

if "%~1"=="" if exist "%GUI_SCRIPT%" (
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%GUI_SCRIPT%"
  exit /b %ERRORLEVEL%
)

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%CLI_SCRIPT%" %*
set "EXIT_CODE=%ERRORLEVEL%"

echo.
if not "%EXIT_CODE%"=="0" echo BrainTrade Demo Acceptance exited with code %EXIT_CODE%.
pause
exit /b %EXIT_CODE%
