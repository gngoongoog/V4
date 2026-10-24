@echo off
setlocal EnableDelayedExpansion
set "SCRIPT_DIR=%~dp0"
if exist "%SCRIPT_DIR%Tools\Compile-Sniper-EA.ps1" (
  set "SCRIPT=%SCRIPT_DIR%Tools\Compile-Sniper-EA.ps1"
) else (
  set "SCRIPT=%SCRIPT_DIR%Compile-Sniper-EA.ps1"
)
if "%~1"=="" (
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT%" -InspectOnly
  echo.
  echo To actually compile: supply -MetaEditorPath and -TerminalDataDir.
  echo See NATIVE_EA_ACCEPTANCE_AR.md. No orders or MT5 settings were changed.
  set "RESULT=!ERRORLEVEL!"
  pause
  exit /b !RESULT!
)
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT%" %*
set "RESULT=%ERRORLEVEL%"
if not "%RESULT%"=="0" echo MetaEditor build was not verified. Read acceptance-output\native-ea\metaeditor-compile.json.
pause
exit /b %RESULT%
